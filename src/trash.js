const TAURI = window.__TAURI__ || null;
const invoke = TAURI ? TAURI.core.invoke : async () => [];

const listEl = document.getElementById("list");
const countEl = document.getElementById("count");

const COLOR = {
  paper: "#D9B84C",
  mint: "#6E9C7A",
  mist: "#6F9CC4",
  blush: "#DFA2A6",
  stone: "#A8A69C",
  ink: "#E8C56A",
};

let notes = [];

function plain(html) {
  const d = document.createElement("div");
  d.innerHTML = html || "";
  return (d.textContent || "").replace(/\s+/g, " ").trim();
}

function ago(ms) {
  if (!ms) return "";
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
  countEl.textContent = notes.length ? `${notes.length} 条` : "空";
  listEl.innerHTML = "";

  if (!notes.length) {
    const d = document.createElement("div");
    d.className = "empty";
    d.textContent = "回收站是空的";
    listEl.appendChild(d);
    return;
  }

  for (const n of notes) {
    const row = document.createElement("div");
    row.className = "row";

    const dot = document.createElement("span");
    dot.className = "d";
    dot.style.background = COLOR[n.color] || COLOR.paper;

    const txt = document.createElement("div");
    txt.className = "txt";
    const t = document.createElement("div");
    t.className = "t";
    const content = plain(n.body);
    t.textContent = content ? content.slice(0, 30) : "空便签";
    const m = document.createElement("div");
    m.className = "m";
    m.textContent = `删除于 ${ago(n.deletedAt)}`;
    txt.append(t, m);

    const acts = document.createElement("div");
    acts.className = "acts";

    const btnBack = document.createElement("button");
    btnBack.textContent = "恢复";
    btnBack.addEventListener("click", () => invoke("restore_note", { id: n.id }));

    const btnDel = document.createElement("button");
    btnDel.className = "danger";
    btnDel.textContent = "彻底删除";
    btnDel.addEventListener("click", () => {
      if (confirm("彻底删除后无法找回，确定？")) {
        invoke("purge_note", { id: n.id });
      }
    });

    acts.append(btnBack, btnDel);
    row.append(dot, txt, acts);
    listEl.appendChild(row);
  }
}

async function refresh() {
  notes = await invoke("list_deleted");
  render();
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && TAURI) TAURI.window.getCurrentWindow().hide();
});

if (TAURI) TAURI.event.listen("trash-changed", refresh);

refresh();
