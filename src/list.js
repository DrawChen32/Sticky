/* 不在 Tauri 里时退化成假数据，方便在浏览器里看样式 */
const TAURI = window.__TAURI__ || null;
const DEMO_NOTES = [
  { id: "1", body: "1. 把 Q4 复盘的结构搭出来 2. 跟设计确认图标风格", color: "paper", updatedAt: Date.now() - 12 * 60000 },
  { id: "2", body: "买菜清单 · 别忘了牛奶", color: "mint", updatedAt: Date.now() - 62 * 60000 },
  { id: "3", body: "会议记录 11/11 周三的复盘会挪到周四", color: "mist", updatedAt: Date.now() - 4 * 3600000 },
  { id: "4", body: "今晚三件事 1. 改完深色桌面 2. 导出 PNG", color: "ink", updatedAt: Date.now() - 26 * 3600000 },
  { id: "5", body: "京东 · 键盘手托", color: "blush", updatedAt: Date.now() - 30 * 3600000 },
];
const invoke = TAURI
  ? TAURI.core.invoke
  : async (cmd) => (cmd === "list_notes" ? DEMO_NOTES : null);

const listEl = document.getElementById("list");
const countEl = document.getElementById("count");
const qEl = document.getElementById("q");

const COLOR = {
  paper: { name: "米黄", hex: "#D9B84C" },
  mint: { name: "薄荷", hex: "#6E9C7A" },
  mist: { name: "雾蓝", hex: "#6F9CC4" },
  blush: { name: "藕粉", hex: "#DFA2A6" },
  stone: { name: "灰石", hex: "#A8A69C" },
  ink: { name: "墨黑", hex: "#E8C56A" },
};

let notes = [];
let keyword = "";

function plain(html) {
  const d = document.createElement("div");
  d.innerHTML = html || "";
  return (d.textContent || "").replace(/\s+/g, " ").trim();
}

function ago(ms) {
  if (!ms) return "刚刚";
  const diff = Date.now() - ms;
  const min = Math.floor(diff / 60000);
  if (min < 1) return "刚刚";
  if (min < 60) return `${min} 分钟前`;
  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} 小时前`;
  const day = Math.floor(hour / 24);
  if (day === 1) return "昨天";
  if (day < 30) return `${day} 天前`;
  return new Date(ms).toLocaleDateString("zh-CN");
}

function render() {
  const kw = keyword.trim().toLowerCase();
  const rows = notes.filter((n) => !kw || plain(n.body).toLowerCase().includes(kw));
  countEl.textContent = `${rows.length} 条`;
  listEl.innerHTML = "";

  if (!rows.length) {
    const d = document.createElement("div");
    d.className = "empty";
    d.textContent = kw ? "没有匹配的便签" : "还没有便签，按 Ctrl + Alt + N 新建";
    listEl.appendChild(d);
    return;
  }

  for (const n of rows) {
    const row = document.createElement("div");
    row.className = "row";

    const dot = document.createElement("span");
    dot.className = "d";
    dot.style.background = (COLOR[n.color] || COLOR.paper).hex;

    const txt = document.createElement("div");
    txt.className = "txt";

    const t = document.createElement("div");
    t.className = "t";
    const content = plain(n.body);
    t.textContent = content ? content.slice(0, 40) : "空便签";

    const m = document.createElement("div");
    m.className = "m";
    m.textContent = `${(COLOR[n.color] || COLOR.paper).name} · ${ago(n.updatedAt)}`;

    txt.append(t, m);
    row.append(dot, txt);
    row.addEventListener("click", () => invoke("focus_note", { id: n.id }));
    listEl.appendChild(row);
  }
}

async function refresh() {
  notes = await invoke("list_notes");
  render();
}

qEl.addEventListener("input", () => {
  keyword = qEl.value;
  render();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && TAURI) TAURI.window.getCurrentWindow().hide();
});

if (TAURI) TAURI.event.listen("notes-changed", refresh);

refresh();
