mod store;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use store::{is_blank, now_ms, Note, Store};

pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub dir: PathBuf,
    pub dirty: Arc<AtomicBool>,
    pub quitting: Arc<AtomicBool>,
}

const COLLAPSED_H: f64 = 32.0;

// ---------------------------------------------------------------- 窗口

fn spawn_note(app: &AppHandle, note: &Note) -> tauri::Result<()> {
    if app.get_webview_window(&note.id).is_some() {
        return Ok(());
    }
    let url = WebviewUrl::App(format!("note.html?id={}", note.id).into());
    let win = WebviewWindowBuilder::new(app, note.id.as_str(), url)
        .title("便利贴")
        .inner_size(note.w, if note.collapsed { COLLAPSED_H } else { note.h })
        .min_inner_size(180.0, COLLAPSED_H)
        .position(note.x, note.y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(true)
        .always_on_top(note.pinned)
        .visible(false)
        .build()?;
    let _ = win.show();
    attach_events(app, note.id.clone());
    Ok(())
}

fn attach_events(app: &AppHandle, label: String) {
    let handle = app.clone();
    if let Some(win) = app.get_webview_window(&label) {
        let l = label.clone();
        win.on_window_event(move |event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => sync_geometry(&handle, &l),
            WindowEvent::Destroyed => {
                let st = handle.state::<AppState>();
                if !st.quitting.load(Ordering::SeqCst) {
                    if let Ok(mut s) = st.store.lock() {
                        if let Some(n) = s.notes.iter_mut().find(|n| n.id == l) {
                            if n.deleted_at == 0 {
                                n.deleted_at = now_ms();
                            }
                        }
                    }
                    st.dirty.store(true, Ordering::SeqCst);
                    let _ = handle.emit("notes-changed", ());
                }
            }
            _ => {}
        });
    }
}

fn sync_geometry(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    let (Ok(pos), Ok(size), Ok(scale)) = (
        win.outer_position(),
        win.outer_size(),
        win.scale_factor(),
    ) else {
        return;
    };
    let p = pos.to_logical::<f64>(scale);
    let s = size.to_logical::<f64>(scale);

    let st = app.state::<AppState>();
    if let Ok(mut store) = st.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == label) {
            n.x = p.x.round();
            n.y = p.y.round();
            if !n.collapsed {
                n.w = s.width.round();
                n.h = s.height.round();
            }
        }
    }
    st.dirty.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------- 命令

#[tauri::command]
fn list_notes(state: tauri::State<'_, AppState>) -> Vec<Note> {
    let Ok(store) = state.store.lock() else {
        return vec![];
    };
    let mut v: Vec<Note> = store.live().cloned().collect();
    v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    v
}

#[tauri::command]
fn get_note(state: tauri::State<'_, AppState>, id: String) -> Option<Note> {
    let Ok(store) = state.store.lock() else {
        return None;
    };
    let found = store.live().find(|n| n.id == id).cloned();
    found
}

/// 在 Windows 上，从同步的命令/菜单回调里直接建 WebView2 窗口会死锁
/// （tauri 文档明确提到）。凡是可能从 UI 回调里触发的建窗，都丢到独立线程。
fn spawn_thread<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::spawn(f);
}

#[tauri::command]
fn create_note(app: AppHandle, state: tauri::State<'_, AppState>) -> Option<Note> {
    let note = {
        let Ok(store) = state.store.lock() else {
            return None;
        };
        let count = store.live().count();
        let step = (count % 5) as f64;
        let mut n = Note::new(&store.settings);
        n.x = 150.0 + step * 32.0;
        n.y = 150.0 + step * 32.0;
        n
    };
    if let Ok(mut store) = state.store.lock() {
        store.notes.push(note.clone());
    }
    state.dirty.store(true, Ordering::SeqCst);
    let handle = app.clone();
    let n = note.clone();
    spawn_thread(move || {
        let _ = spawn_note(&handle, &n);
    });
    let _ = app.emit("notes-changed", ());
    Some(note)
}

#[tauri::command]
fn save_body(app: AppHandle, state: tauri::State<'_, AppState>, id: String, body: String) {
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            if n.body != body {
                n.body = body;
                n.updated_at = now_ms();
            }
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit("notes-changed", ());
}

#[tauri::command]
fn set_color(app: AppHandle, state: tauri::State<'_, AppState>, id: String, color: String) {
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            n.color = color;
            n.updated_at = now_ms();
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit("note-state", id);
}

#[tauri::command]
fn set_font_size(app: AppHandle, state: tauri::State<'_, AppState>, id: String, size: u32) {
    let size = size.clamp(11, 20);
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            n.font_size = size;
            n.updated_at = now_ms();
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit("note-state", id);
}

#[tauri::command]
fn set_flag(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    flag: String,
    value: bool,
) {
    let mut resize: Option<(f64, f64)> = None;
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            match flag.as_str() {
                "pinned" => n.pinned = value,
                "ghost" => n.ghost = value,
                "collapsed" => {
                    n.collapsed = value;
                    resize = Some((n.w, if value { COLLAPSED_H } else { n.h }));
                }
                _ => {}
            }
            n.updated_at = now_ms();
        }
    }

    if let Some(win) = app.get_webview_window(&id) {
        if let Some((w, h)) = resize {
            let _ = win.set_size(LogicalSize::new(w, h));
        }
        if flag == "pinned" {
            let _ = win.set_always_on_top(value);
        }
    }

    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit("note-state", id);
}

#[tauri::command]
fn delete_note(app: AppHandle, state: tauri::State<'_, AppState>, id: String) {
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            // 空便签直接丢弃；有内容的进回收站（30 天后清理，见 v0.2）
            n.deleted_at = if is_blank(&n.body) { u64::MAX } else { now_ms() };
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    if let Some(win) = app.get_webview_window(&id) {
        let _ = win.close();
    }
    let _ = app.emit("notes-changed", ());
}

#[tauri::command]
fn focus_note(app: AppHandle, id: String) {
    if let Some(win) = app.get_webview_window(&id) {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn list_deleted(state: tauri::State<'_, AppState>) -> Vec<Note> {
    let Ok(store) = state.store.lock() else {
        return vec![];
    };
    let mut v: Vec<Note> = store.deleted().cloned().collect();
    v.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    v
}

#[tauri::command]
fn restore_note(app: AppHandle, state: tauri::State<'_, AppState>, id: String) {
    let mut restored: Option<Note> = None;
    if let Ok(mut store) = state.store.lock() {
        if let Some(n) = store.notes.iter_mut().find(|n| n.id == id) {
            n.deleted_at = 0;
            n.updated_at = now_ms();
            restored = Some(n.clone());
        }
    }
    state.dirty.store(true, Ordering::SeqCst);
    if let Some(n) = restored {
        let handle = app.clone();
        spawn_thread(move || {
            let _ = spawn_note(&handle, &n);
        });
    }
    let _ = app.emit("notes-changed", ());
    let _ = app.emit("trash-changed", ());
}

#[tauri::command]
fn purge_note(app: AppHandle, state: tauri::State<'_, AppState>, id: String) {
    if let Ok(mut store) = state.store.lock() {
        store.notes.retain(|n| n.id != id);
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = app.emit("trash-changed", ());
}

#[tauri::command]
fn tidy_notes(app: AppHandle) {
    tidy_from_tray(&app);
}

// ---------------------------------------------------------------- 托盘

fn new_note_quick(app: &AppHandle) -> Option<Note> {
    let state = app.state::<AppState>();
    let note = {
        let Ok(store) = state.store.lock() else {
            return None;
        };
        let step = (store.live().count() % 5) as f64;
        let mut n = Note::new(&store.settings);
        n.x = 150.0 + step * 32.0;
        n.y = 150.0 + step * 32.0;
        n
    };
    if let Ok(mut store) = state.store.lock() {
        store.notes.push(note.clone());
    }
    state.dirty.store(true, Ordering::SeqCst);
    let _ = spawn_note(app, &note);
    let _ = app.emit("notes-changed", ());
    Some(note)
}

fn open_list(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("list") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, "list", WebviewUrl::App("list.html".into()))
        .title("便利贴")
        .inner_size(340.0, 480.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .always_on_top(true)
        .center()
        .build()?;
    let _ = win.set_focus();
    Ok(())
}

fn open_trash(app: &AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("trash") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, "trash", WebviewUrl::App("trash.html".into()))
        .title("回收站")
        .inner_size(420.0, 460.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .always_on_top(true)
        .center()
        .build()?;
    let _ = win.set_focus();
    Ok(())
}

fn toggle_all(app: &AppHandle) {
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let Ok(store) = state.store.lock() else {
            return;
        };
        let collected: Vec<String> = store.live().map(|n| n.id.clone()).collect();
        collected
    };
    let mut any_visible = false;
    for id in &ids {
        if let Some(win) = app.get_webview_window(id) {
            if win.is_visible().unwrap_or(false) {
                any_visible = true;
                break;
            }
        }
    }
    for id in ids {
        if let Some(win) = app.get_webview_window(&id) {
            if any_visible {
                let _ = win.hide();
            } else {
                let _ = win.show();
            }
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("new", "新建便签")
        .text("all", "全部便签")
        .text("toggle", "显示 / 隐藏全部")
        .text("tidy", "整理桌面便签")
        .text("trash", "回收站")
        .separator()
        .text("quit", "退出")
        .build()?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("便利贴")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let action = event.id().0.clone();
            let handle = app.clone();
            match action.as_str() {
                "new" => spawn_thread(move || {
                    let _ = new_note_quick(&handle);
                }),
                "all" => spawn_thread(move || {
                    let _ = open_list(&handle);
                }),
                "toggle" => toggle_all(&handle),
                "tidy" => tidy_from_tray(&handle),
                "trash" => spawn_thread(move || {
                    let _ = open_trash(&handle);
                }),
                "quit" => {
                    flush_now(&handle);
                    handle
                        .state::<AppState>()
                        .quitting
                        .store(true, Ordering::SeqCst);
                    handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let handle = tray.app_handle().clone();
                spawn_thread(move || {
                    let _ = open_list(&handle);
                });
            }
        })
        .build(app)?;
    Ok(())
}

fn tidy_from_tray(app: &AppHandle) {
    let mut items: Vec<(String, f64)> = vec![];
    {
        let state = app.state::<AppState>();
        if let Ok(store) = state.store.lock() {
            for n in store.live() {
                items.push((n.id.clone(), n.w));
            }
        };
    }
    let mut x = 120.0;
    let y = 140.0;
    for (id, w) in items {
        if let Some(win) = app.get_webview_window(&id) {
            let _ = win.set_position(LogicalPosition::new(x, y));
        }
        x += w + 16.0;
    }
}

// ---------------------------------------------------------------- 快捷键 & 落盘

fn register_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.global_shortcut()
        .on_shortcut("CmdOrCtrl+Alt+N", |app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                let handle = app.clone();
                spawn_thread(move || {
                    let _ = new_note_quick(&handle);
                });
            }
        })?;
    Ok(())
}

fn flush_now(app: &AppHandle) {
    let state = app.state::<AppState>();
    let snapshot = match state.store.lock() {
        Ok(s) => s.clone(),
        Err(_) => return,
    };
    let _ = snapshot.save(&state.dir);
    state.dirty.store(false, Ordering::SeqCst);
}

fn start_flusher(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(700));
        let state = app.state::<AppState>();
        if state.dirty.swap(false, Ordering::SeqCst) {
            let snapshot = match state.store.lock() {
                Ok(s) => s.clone(),
                Err(_) => continue,
            };
            let _ = snapshot.save(&state.dir);
        }
    });
}

// ---------------------------------------------------------------- 启动

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&dir).ok();

            let loaded = Store::load(&dir);
            if loaded.settings.auto_backup && !loaded.notes.is_empty() {
                let _ = loaded.backup_today(&dir);
            }
            let initial: Vec<Note> = loaded.live().cloned().collect();

            app.manage(AppState {
                store: Arc::new(Mutex::new(loaded.clone())),
                dir,
                dirty: Arc::new(AtomicBool::new(false)),
                quitting: Arc::new(AtomicBool::new(false)),
            });

            let handle = app.handle().clone();
            if initial.is_empty() {
                let _ = new_note_quick(&handle);
            } else {
                for n in &initial {
                    let _ = spawn_note(&handle, n);
                }
            }

            build_tray(&handle)?;
            register_hotkeys(&handle)?;
            start_flusher(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_notes,
            get_note,
            create_note,
            save_body,
            set_color,
            set_font_size,
            set_flag,
            delete_note,
            list_deleted,
            restore_note,
            purge_note,
            focus_note,
            tidy_notes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
