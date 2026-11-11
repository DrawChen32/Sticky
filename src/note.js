/* 不在 Tauri 里时（直接双击 HTML 看样式），退化成一份假数据，方便调视觉。
   参数：?c=mist&edit=1&collapsed=1&ghost=1&empty=1 */
const TAURI = window.__TAURI__ || null;
const Q = new URLSearchParams(location.search);
const DEMO = {
  id: "demo",
  body:
    Q.get("empty") === "1"
      ? ""
      : "<div>1. 把 Q4 复盘的结构搭出来</div><div>2. 跟设计确认图标风格</div><div>3. 下班前回老陈的邮件</div>",
  color: Q.get("c") || "paper",
  fontSize: 13,
  collapsed: Q.get("collapsed") === "1",
  pinned: Q.get("pin") === "1",
  ghost: Q.get("ghost") === "1",
};
const invoke = TAURI
  ? TAURI.core.invoke
  : async (cmd) => (cmd === "get_note" ? DEMO : null);

const id = Q.get("id") || "demo";
const $ = (s) => document.querySelector(s);

const bodyEl = $("#body");
const ttlEl = $("#ttl");
const pinBtn = $("#pin");
const ghostBtn = $("#ghost");
const foldBtn = $("#fold");

let note = null;
let saveTimer = null;

function updateTitle() {
  const first = (bodyEl.innerText || "").split("\n")[0].trim();
  ttlEl.textContent = first ? first.slice(0, 28) : "新便签";
}

function applyState(n) {
  note = n;
  document.body.dataset.paper = n.color;
  document.body.dataset.collapsed = String(n.collapsed);
  document.body.dataset.ghost = String(n.ghost);
  document.documentElement.style.setProperty("--fs", `${n.fontSize}px`);
  pinBtn.classList.toggle("on", n.pinned);
  ghostBtn.classList.toggle("on", n.ghost);
  foldBtn.classList.toggle("on", n.collapsed);
  document
    .querySelectorAll(".sw")
    .forEach((el) => el.classList.toggle("sel", el.dataset.c === n.color));
  updateTitle();
}

/* 300ms 防抖：打字过程中不写盘，停手才写 */
function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    if (!id) return;
    invoke("save_body", { id, body: bodyEl.innerHTML }).catch(() => {});
  }, 300);
}

async function boot() {
  if (!id) return;
  const n = await invoke("get_note", { id });
  if (!n) return;
  bodyEl.innerHTML = n.body || "";
  applyState(n);
  if (!TAURI && Q.get("edit") === "1") document.body.dataset.editing = "true";
  if (!n.body) setTimeout(() => bodyEl.focus(), 60);
}

/* ---------------- 正文 ---------------- */
bodyEl.addEventListener("input", () => {
  updateTitle();
  scheduleSave();
});
bodyEl.addEventListener("focus", () => {
  document.body.dataset.editing = "true";
});
bodyEl.addEventListener("blur", () => {
  document.body.dataset.editing = "false";
  scheduleSave();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    bodyEl.blur();
    document.body.dataset.editing = "false";
  }
});

/* ---------------- 工具栏 ---------------- */
document.querySelectorAll(".tools [data-cmd]").forEach((btn) => {
  btn.addEventListener("mousedown", (e) => e.preventDefault());
  btn.addEventListener("click", () => {
    document.execCommand(btn.dataset.cmd, false, null);
    bodyEl.focus();
  });
});

/* 待办：插一个可勾选的块，点方框切换完成态 */
const todoBtn = document.querySelector('[data-act="todo"]');
todoBtn.addEventListener("mousedown", (e) => e.preventDefault());
todoBtn.addEventListener("click", () => {
  const tk = document.createElement("div");
  tk.className = "tk";
  tk.setAttribute("data-done", "0");

  const cb = document.createElement("span");
  cb.className = "cbx";
  cb.setAttribute("contenteditable", "false");

  const tx = document.createElement("span");
  tx.className = "txt";
  tx.appendChild(document.createElement("br"));

  tk.append(cb, tx);

  const sel = window.getSelection();
  if (sel && sel.rangeCount) {
    const range = sel.getRangeAt(0);
    range.deleteContents();
    range.insertNode(tk);
  } else {
    bodyEl.appendChild(tk);
  }

  const after = document.createRange();
  after.selectNodeContents(tx);
  after.collapse(true);
  if (sel) {
    sel.removeAllRanges();
    sel.addRange(after);
  }
  bodyEl.focus();
  updateTitle();
  scheduleSave();
});

bodyEl.addEventListener("click", (e) => {
  const cb = e.target.closest(".cbx");
  if (!cb) return;
  e.preventDefault();
  const tk = cb.parentElement;
  const done = tk.getAttribute("data-done") === "1";
  tk.setAttribute("data-done", done ? "0" : "1");
  scheduleSave();
});

/* Ctrl / Cmd + 滚轮调字号，逐张便签独立 */
document.addEventListener(
  "wheel",
  (e) => {
    if (!(e.ctrlKey || e.metaKey) || !note) return;
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1;
    const next = Math.min(20, Math.max(11, note.fontSize + dir));
    if (next === note.fontSize) return;
    note.fontSize = next;
    document.documentElement.style.setProperty("--fs", `${next}px`);
    invoke("set_font_size", { id, size: next }).catch(() => {});
  },
  { passive: false }
);

document.querySelectorAll(".sw").forEach((btn) => {
  btn.addEventListener("mousedown", (e) => e.preventDefault());
  btn.addEventListener("click", () => {
    invoke("set_color", { id, color: btn.dataset.c }).catch(() => {});
    applyState({ ...note, color: btn.dataset.c });
  });
});

/* ---------------- 标题栏 ---------------- */
function toggleCollapsed() {
  const next = !note.collapsed;
  note.collapsed = next;
  document.body.dataset.collapsed = String(next);
  foldBtn.classList.toggle("on", next);
  invoke("set_flag", { id, flag: "collapsed", value: next }).catch(() => {});
}

foldBtn.addEventListener("click", toggleCollapsed);
document.getElementById("bar").addEventListener("dblclick", (e) => {
  if (e.target.closest("button")) return;
  toggleCollapsed();
});

pinBtn.addEventListener("click", () => {
  const next = !note.pinned;
  note.pinned = next;
  pinBtn.classList.toggle("on", next);
  invoke("set_flag", { id, flag: "pinned", value: next }).catch(() => {});
});

ghostBtn.addEventListener("click", () => {
  const next = !note.ghost;
  note.ghost = next;
  document.body.dataset.ghost = String(next);
  ghostBtn.classList.toggle("on", next);
  invoke("set_flag", { id, flag: "ghost", value: next }).catch(() => {});
});

/* 空便签直接消失；有内容的进回收站（软删除），文件里还留着 */
document.getElementById("close").addEventListener("click", () => {
  clearTimeout(saveTimer);
  invoke("save_body", { id, body: bodyEl.innerHTML })
    .then(() => invoke("delete_note", { id }))
    .catch(() => {});
});

/* 折叠时标题条要能拖动；窗口高度由 Rust 侧改 */
if (TAURI) {
  TAURI.event.listen("note-state", async (payload) => {
    if (payload.payload !== id) return;
    const n = await invoke("get_note", { id });
    if (n) applyState(n);
  });
}

boot();
