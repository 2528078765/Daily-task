const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));
const isTauriApp = typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__ !== "undefined";

if (!isTauriApp) {
  document.body.innerHTML = '<div style="display:flex;align-items:center;justify-content:center;height:100vh;color:#f2f4f5;background:#17191b;font-family:sans-serif;">请使用桌面版启动</div>';
  throw new Error("Desktop only");
}

function invokeTauri(command, args) {
  const api = window.__TAURI__ && window.__TAURI__.core;
  if (api && typeof api.invoke === "function") {
    return api.invoke(command, args);
  }
  if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function") {
    return window.__TAURI_INTERNALS__.invoke(command, args);
  }
  return Promise.reject(new Error("Tauri runtime unavailable"));
}

const PERIODS = [
  { id: "all-day", label: "全天" },
  { id: "morning", label: "上午" },
  { id: "afternoon", label: "下午" },
  { id: "evening", label: "晚上" }
];
const PRIORITY_LABEL = { 0: "低", 1: "中", 2: "高" };
const WEEK = ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"];

const state = {
  tasks: [],
  history: [],
  seeded: false,
  templates: [],
  templatesSeeded: false,
  settings: {
    theme: "dark",
    dock: "full",
    autoCollapse: true,
    view: "today",
    panelColor: null,
    panelOpacity: 100,
    locked: false,
    autostart: false,
    collapseDirection: "right"
  },
  collapsed: false,
  settingsOpen: false,
  editingId: null,
  addPeriod: "all-day",
  addPriority: 1,
  addTime: "",
  addDate: todayKey(),
  timePicker: { hour: 12, minute: 0 },
  activeTemplateId: null,
  expandedTemplates: new Set(),
  expandedHistoryDates: new Set(),
  renamingTemplateId: null
};

let dragId = null;
let pointerDragActive = false;
let collapseTimer = null;
let toastTimer = null;
let dropTarget = null;
let templateToggleTimer = null;
let expandedGraceUntil = 0;

function uid() {
  if (window.crypto && typeof window.crypto.randomUUID === "function") {
    return window.crypto.randomUUID();
  }
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function todayKey(date = new Date()) {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

function dateLabelFromKey(key) {
  const parts = key.split("-").map(Number);
  const date = new Date(parts[0], parts[1] - 1, parts[2]);
  return `${date.getMonth() + 1}月${date.getDate()}日 ${WEEK[date.getDay()]}`;
}

function taskDate(task) {
  return task.date || todayKey(new Date(task.createdAt));
}

function isFutureTask(task) {
  return taskDate(task) > todayKey();
}

function shortDateLabel(key) {
  const parts = key.split("-").map(Number);
  return `${parts[1]}月${parts[2]}日`;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function findTask(id) {
  return state.tasks.find((task) => task.id === id);
}

function seedTasks() {
  return [];
}

function seedHistory() {
  return [];
}

function seedTemplates() {
  return [];
}

function save() {
  const payload = {
    seeded: state.seeded,
    tasks: state.tasks,
    history: state.history,
    templatesSeeded: state.templatesSeeded,
    templates: state.templates,
    settings: state.settings
  };
  const json = JSON.stringify(payload);
  invokeTauri("save_data", { contents: json })
    .catch((error) => console.error("保存数据失败", error));
}

async function load() {
  let data = null;
  try {
    const contents = await invokeTauri("load_data");
    if (contents) data = JSON.parse(contents);
  } catch (error) {
    console.warn("读取本地数据失败", error);
  }

  if (data) {
    state.tasks = Array.isArray(data.tasks) ? data.tasks : [];
    state.seeded = data.seeded === true;
    state.tasks.forEach((task) => {
      if (!task.date) task.date = todayKey(new Date(task.createdAt));
    });
    if (Array.isArray(data.history)) {
      state.history = data.history;
    } else {
      const key = todayKey();
      state.history = state.tasks.filter((task) => taskDate(task) !== key);
      state.tasks = state.tasks.filter((task) => taskDate(task) === key);
      save();
    }
    state.history.forEach((task) => {
      if (!task.date) task.date = todayKey(new Date(task.createdAt));
    });
    state.templates = Array.isArray(data.templates) ? data.templates : [];
    state.templatesSeeded = data.templatesSeeded === true;
    const legacyTemplates = state.templates.filter(
      (item) => item && typeof item.title === "string" && !Array.isArray(item.tasks)
    );
    if (legacyTemplates.length) {
      state.templates = [{
        id: uid(),
        name: "模板1",
        tasks: legacyTemplates.map((item) => ({
          id: uid(),
          title: item.title,
          time: item.time || "",
          period: item.period || "all-day",
          priority: typeof item.priority === "number" ? item.priority : 1
        }))
      }];
      state.templatesSeeded = true;
      save();
    }
    state.settings = Object.assign({}, state.settings, data.settings || {});
    if (data.settings && !data.settings.edgeMode) {
      state.settings.autoCollapse = true;
      state.settings.edgeMode = true;
      save();
    }
  }

  if (!state.seeded) {
    state.tasks = seedTasks();
    state.history = seedHistory();
    state.seeded = true;
    save();
  }

  if (!state.templatesSeeded) {
    state.templates = seedTemplates();
    state.templatesSeeded = true;
    save();
  }

  if (archivePastTasks()) save();

  if (!state.activeTemplateId && state.templates.length) {
    state.activeTemplateId = state.templates[0].id;
  }

  if (isTauriApp) {
    try {
      state.settings.autostart = await invokeTauri("autostart_enabled");
    } catch (error) {
      console.warn("读取开机自启状态失败", error);
    }
  }

  render();
  if (isTauriApp) {
    await invokeTauri("set_locked", { locked: state.settings.locked })
      .catch((error) => console.warn("设置锁定状态失败", error));
    await applyDesktopWindow();
  }
}

function nextSort() {
  return Math.max(0, ...state.tasks.map((task) => task.sort)) + 10;
}

function archivePastTasks() {
  const key = todayKey();
  const past = state.tasks.filter((task) => taskDate(task) < key);
  if (!past.length) return false;
  past.forEach((task) => {
    if (!task.date) task.date = taskDate(task);
  });
  state.history.push(...past);
  state.tasks = state.tasks.filter((task) => taskDate(task) >= key);
  return true;
}

function parseInput(raw, explicitTime = "") {
  let title = raw.trim();
  let time = explicitTime.trim() || "";

  if (!time) {
    const timeMatch = title.match(/(?:^|\s)([01]?\d|2[0-3]):([0-5]\d)\s*/);
    if (timeMatch) {
      time = `${timeMatch[1].padStart(2, "0")}:${timeMatch[2]}`;
      title = title.replace(timeMatch[0], " ").trim();
    }
  }

  let priority = state.addPriority;
  const priorityMatch = title.match(/#(高|中|低)/);
  if (priorityMatch) {
    priority = { 高: 2, 中: 1, 低: 0 }[priorityMatch[1]];
    title = title.replace(priorityMatch[0], " ").trim();
  }

  let period = state.addPeriod;
  if (time) {
    const hour = Number(time.split(":")[0]);
    period = hour < 12 ? "morning" : hour < 18 ? "afternoon" : "evening";
  }

  if (!title) {
    title = "未命名任务";
  }

  return { title, time, period, priority };
}

function addTask() {
  const input = $("#taskInput");
  const raw = input.value;
  if (!raw.trim()) {
    input.focus();
    return;
  }

  const parsed = parseInput(raw, state.addTime);
  const task = {
    id: uid(),
    title: parsed.title,
    time: parsed.time,
    period: parsed.period,
    priority: parsed.priority,
    done: false,
    date: state.addDate,
    createdAt: new Date().toISOString(),
    sort: nextSort()
  };

  state.tasks.push(task);
  state.addPeriod = "all-day";
  state.addTime = "";
  state.addDate = todayKey();
  input.value = "";
  save();
  render();

  requestAnimationFrame(() => {
    const scroll = $("#taskScroll");
    scroll.scrollTop = scroll.scrollHeight;
    input.focus();
  });
}

function addTemplate() {
  const input = $("#taskInput");
  const raw = input.value;
  if (!raw.trim()) {
    input.focus();
    return;
  }

  let template = state.templates.find((item) => item.id === state.activeTemplateId);
  if (!template) {
    template = { id: uid(), name: templateNameForNew(), tasks: [] };
    state.templates.push(template);
    state.activeTemplateId = template.id;
    state.expandedTemplates.add(template.id);
  }

  const parsed = parseInput(raw, state.addTime);
  template.tasks.push({
    id: uid(),
    title: parsed.title,
    time: parsed.time,
    period: parsed.period,
    priority: parsed.priority
  });
  state.templatesSeeded = true;
  state.addPeriod = "all-day";
  state.addTime = "";
  input.value = "";
  save();
  render();
  toast(`已添加到 ${template.name}`);
  requestAnimationFrame(() => input.focus());
}

function toggleTask(id) {
  const task = findTask(id);
  if (!task || isFutureTask(task)) return;
  task.done = !task.done;
  save();
  render();
}

function deleteTask(id) {
  state.tasks = state.tasks.filter((task) => task.id !== id);
  save();
  render();
}

function deleteTemplate(id) {
  state.templates = state.templates.filter((template) => template.id !== id);
  state.expandedTemplates.delete(id);
  if (state.activeTemplateId === id) {
    state.activeTemplateId = state.templates.length ? state.templates[0].id : null;
  }
  state.templatesSeeded = true;
  save();
  render();
}

function normalizedTitle(value) {
  return String(value).trim().toLowerCase();
}

function templateNameForNew() {
  const maxNumber = state.templates.reduce((max, template) => {
    const match = String(template.name || "").match(/^模板(\d+)$/);
    return match ? Math.max(max, Number(match[1])) : max;
  }, 0);
  return `模板${maxNumber + 1}`;
}

function findTemplate(id) {
  return state.templates.find((template) => template.id === id);
}

function addTemplateTaskToToday(task) {
  const key = todayKey();
  const todayTitles = new Set(
    state.tasks
      .filter((item) => taskDate(item) === key)
      .map((item) => normalizedTitle(item.title))
  );
  if (todayTitles.has(normalizedTitle(task.title))) return false;
  state.tasks.push({
    id: uid(),
    title: task.title,
    time: task.time,
    period: task.period,
    priority: task.priority,
    done: false,
    date: todayKey(),
    createdAt: new Date().toISOString(),
    sort: nextSort()
  });
  return true;
}

function applyTemplates(templateId = null) {
  const templates = templateId
    ? state.templates.filter((template) => template.id === templateId)
    : state.templates;
  const templateTasks = templates.flatMap((template) => template.tasks || []);
  if (!templateTasks.length) {
    toast("还没有模板任务");
    return;
  }

  let added = 0;
  templateTasks.forEach((task) => {
    if (addTemplateTaskToToday(task)) added += 1;
  });

  state.settings.view = "today";
  save();
  render();
  toast(added ? `已添加 ${added} 项模板任务` : "今天的任务已齐全");
}

function applyTemplateTask(templateId, taskId) {
  const template = findTemplate(templateId);
  const task = template ? template.tasks.find((item) => item.id === taskId) : null;
  if (!task) return;

  const added = addTemplateTaskToToday(task);
  state.settings.view = "today";
  save();
  render();
  toast(added ? `已添加 ${task.title}` : "今天的任务已齐全");
}

function updateTemplateFromToday(templateId) {
  const template = findTemplate(templateId);
  if (!template) return;
  const key = todayKey();
  const todayTasks = state.tasks
    .filter((task) => taskDate(task) === key)
    .map((task) => ({
      id: uid(),
      title: task.title,
      time: task.time,
      period: task.period,
      priority: task.priority
    }));
  template.tasks = todayTasks;
  state.templatesSeeded = true;
  save();
  render();
  toast(`${template.name} 已更新为今天的任务`);
}

function createTemplateFromToday() {
  const key = todayKey();
  const todayTasks = state.tasks.filter((task) => taskDate(task) === key);
  if (!todayTasks.length) {
    toast("今天还没有任务");
    return;
  }

  const template = {
    id: uid(),
    name: templateNameForNew(),
    tasks: todayTasks.map((task) => ({
      id: uid(),
      title: task.title,
      time: task.time,
      period: task.period,
      priority: task.priority
    }))
  };
  state.templates.push(template);
  state.activeTemplateId = template.id;
  state.expandedTemplates.add(template.id);
  state.templatesSeeded = true;
  save();
  render();
  toast(`已创建 ${template.name}`);
}

function deleteTemplateTask(templateId, taskId) {
  const template = findTemplate(templateId);
  if (!template) return;
  template.tasks = template.tasks.filter((task) => task.id !== taskId);
  state.templatesSeeded = true;
  save();
  render();
}

function toggleTemplate(templateId) {
  state.activeTemplateId = templateId;
  if (state.expandedTemplates.has(templateId)) {
    state.expandedTemplates.delete(templateId);
  } else {
    state.expandedTemplates.add(templateId);
  }
  render();
}

function startRenameTemplate(templateId) {
  state.renamingTemplateId = templateId;
  render();
  requestAnimationFrame(() => {
    const input = $(".template-rename-input");
    if (input) {
      input.focus();
      input.select();
    }
  });
}

function commitTemplateRename(templateId, value) {
  if (state.renamingTemplateId !== templateId) return;
  const template = findTemplate(templateId);
  state.renamingTemplateId = null;
  if (template && value.trim()) {
    template.name = value.trim();
  }
  save();
  render();
}

function cancelTemplateRename() {
  state.renamingTemplateId = null;
  render();
}

function toast(message) {
  const element = $("#toast");
  element.textContent = message;
  element.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => element.classList.remove("show"), 1800);
}

function moveToIndex(dragTaskId, period, index) {
  const drag = findTask(dragTaskId);
  if (!drag) return;

  const siblings = state.tasks
    .filter((task) => task.period === period && task.id !== dragTaskId)
    .sort((a, b) => a.sort - b.sort);
  const before = siblings[index - 1];
  const after = siblings[index];

  drag.period = period;
  if (before && after) {
    drag.sort = (before.sort + after.sort) / 2;
  } else if (after) {
    drag.sort = after.sort - 10;
  } else {
    drag.sort = (before ? before.sort : 0) + 10;
  }
  save();
  render();
}

function byOrder(a, b) {
  return a.sort - b.sort || new Date(a.createdAt) - new Date(b.createdAt);
}

function rowHtml(task) {
  const editing = state.editingId === task.id;
  const future = isFutureTask(task);
  const title = editing
    ? `<input class="edit-input" value="${escapeHtml(task.title)}" maxlength="120" data-edit-id="${task.id}">`
    : `<span class="task-title">${escapeHtml(task.title)}</span>`;
  const timeText = task.time
    ? (future ? `${shortDateLabel(taskDate(task))} ${task.time}` : task.time)
    : (future ? shortDateLabel(taskDate(task)) : "");
  const time = timeText
    ? `<span class="task-time"><i data-lucide="clock" class="icon-12"></i>${timeText}</span>`
    : "";
  const date = state.settings.view === "history"
    ? `<span class="task-date">${dateLabelFromKey(todayKey(new Date(task.createdAt)))}</span>`
    : "";

  return `
    <div class="task-row ${task.done ? "done" : ""} ${future ? "future" : ""} ${editing ? "editing" : ""}" data-id="${task.id}" data-period="${task.period}">
      <button class="check" data-action="toggle" ${future ? "disabled" : ""} aria-label="完成任务" aria-pressed="${task.done}" title="${future ? "未来待办暂不能完成" : (task.done ? "标记未完成" : "标记完成")}">
        <i data-lucide="check" class="icon-13"></i>
      </button>
      <span class="priority" data-level="${task.priority}" title="优先级 ${PRIORITY_LABEL[task.priority]}"><i></i><b>${PRIORITY_LABEL[task.priority]}</b></span>
      <div class="task-main">${title}${time}${date}</div>
      <button class="icon-btn grip" data-action="drag" aria-label="拖动排序" title="拖动排序"><i data-lucide="grip-vertical" class="icon-14"></i></button>
      <button class="icon-btn delete" data-action="delete" aria-label="删除任务" title="删除任务"><i data-lucide="trash-2" class="icon-14"></i></button>
    </div>`;
}

function groupHtml(period, list, done) {
  const emptyClass = list.length ? "" : " empty-list";
  return `
    <section class="group" data-period="${period.id}">
      <div class="group-header"><span>${period.label}</span><span class="count">${done}/${list.length}</span></div>
      <div class="task-list${emptyClass}" data-period="${period.id}">${list.map(rowHtml).join("")}</div>
    </section>`;
}

function emptyHtml(type) {
  const icon = type === "today" ? "list-checks" : "history";
  const title = type === "today" ? "今天还没有任务" : "还没有历史记录";
  return `
    <div class="empty">
      <i data-lucide="${icon}"></i>
      <span class="empty-title">${title}</span>
    </div>`;
}

function renderToday() {
  const key = todayKey();
  const todayTasks = state.tasks.filter((task) => taskDate(task) >= key);
  const wrap = $("#taskLists");
  wrap.innerHTML = "";

  let renderedGroups = 0;
  PERIODS.forEach((period) => {
    const list = todayTasks.filter((task) => task.period === period.id).sort(byOrder);
    if (!list.length) {
      wrap.insertAdjacentHTML(
        "beforeend",
        `<div class="task-list empty-list" data-period="${period.id}"><span class="empty-label">${period.label} · 暂无任务</span></div>`
      );
      renderedGroups += 1;
      return;
    }
    renderedGroups += 1;
    const done = list.filter((task) => task.done).length;
    wrap.insertAdjacentHTML("beforeend", groupHtml(period, list, done));
  });

  if (!renderedGroups) {
    wrap.innerHTML = emptyHtml("today");
  }
}

function historyRowHtml(task) {
  const time = task.time
    ? `<span class="task-time"><i data-lucide="clock" class="icon-12"></i>${task.time}</span>`
    : "";
  const status = task.done ? "已完成" : "未完成";
  return `
    <div class="task-row history-row ${task.done ? "done" : ""}">
      <span class="history-status ${task.done ? "done" : ""}">${status}</span>
      <span class="priority" data-level="${task.priority}" title="优先级 ${PRIORITY_LABEL[task.priority]}"><i></i><b>${PRIORITY_LABEL[task.priority]}</b></span>
      <div class="task-main"><span class="task-title">${escapeHtml(task.title)}</span>${time}</div>
    </div>`;
}

function historyCardHtml(dateKey, list) {
  const expanded = state.expandedHistoryDates.has(dateKey);
  const done = list.filter((task) => task.done).length;
  const body = expanded ? `
    <div class="template-card-body">
      <div class="task-list" data-period="history">${list.map(historyRowHtml).join("")}</div>
    </div>` : "";
  return `
    <section class="group template-card history-card" data-history-date="${dateKey}">
      <button class="template-card-header" data-action="toggle-history-date" data-date="${dateKey}">
        <i data-lucide="${expanded ? "chevron-down" : "chevron-right"}" class="icon-14"></i>
        <span class="history-date-name">${dateLabelFromKey(dateKey)}</span>
        <span class="count">已完成 ${done}/${list.length}</span>
      </button>
      ${body}
    </section>`;
}

function toggleHistoryDate(dateKey) {
  if (state.expandedHistoryDates.has(dateKey)) {
    state.expandedHistoryDates.delete(dateKey);
  } else {
    state.expandedHistoryDates.add(dateKey);
  }
  render();
}

function renderHistory() {
  const pastTasks = [...state.history].sort((a, b) => taskDate(b).localeCompare(taskDate(a)));
  const wrap = $("#taskLists");
  wrap.innerHTML = "";

  if (!pastTasks.length) {
    wrap.innerHTML = emptyHtml("history");
    return;
  }

  const groups = new Map();
  pastTasks.forEach((task) => {
    const groupKey = taskDate(task);
    if (!groups.has(groupKey)) groups.set(groupKey, []);
    groups.get(groupKey).push(task);
  });

  groups.forEach((list, groupKey) => {
    wrap.insertAdjacentHTML("beforeend", historyCardHtml(groupKey, list));
  });
}

function templateTaskRowHtml(templateId, task) {
  const time = task.time
    ? `<span class="task-time"><i data-lucide="clock" class="icon-12"></i>${task.time}</span>`
    : "";
  return `
    <div class="task-row template-row" data-template-task-id="${task.id}">
      <span class="priority" data-level="${task.priority}" title="优先级 ${PRIORITY_LABEL[task.priority]}"><i></i><b>${PRIORITY_LABEL[task.priority]}</b></span>
      <div class="task-main"><span class="task-title">${escapeHtml(task.title)}</span>${time}</div>
      <button class="icon-btn copy-btn" data-action="apply-template-task" data-template-id="${templateId}" data-task-id="${task.id}" aria-label="添加到今天" title="添加到今天"><i data-lucide="copy-plus" class="icon-14"></i></button>
      <button class="icon-btn delete" data-action="delete-template-task" data-template-id="${templateId}" data-task-id="${task.id}" aria-label="删除模板任务" title="删除模板任务"><i data-lucide="trash-2" class="icon-14"></i></button>
    </div>`;
}

function templateCardHtml(template) {
  const expanded = state.expandedTemplates.has(template.id);
  const active = state.activeTemplateId === template.id;
  const nameHtml = state.renamingTemplateId === template.id
    ? `<input class="template-rename-input" data-rename-id="${template.id}" value="${escapeHtml(template.name)}" maxlength="20">`
    : `<span class="template-name">${escapeHtml(template.name)}</span>`;
  const body = expanded ? `
    <div class="template-card-body">
      <div class="template-card-actions">
        <button class="mini-btn" data-action="update-template-from-today" data-template-id="${template.id}"><i data-lucide="download"></i>从今天更新</button>
        <button class="mini-btn primary" data-action="apply-template" data-template-id="${template.id}"><i data-lucide="copy-check"></i>应用到今天</button>
        <button class="mini-btn" data-action="rename-template" data-template-id="${template.id}"><i data-lucide="pencil"></i>重命名</button>
        <button class="mini-btn danger" data-action="delete-template" data-template-id="${template.id}"><i data-lucide="trash-2"></i>删除</button>
      </div>
      <div class="task-list" data-period="templates">
        ${template.tasks.map((task) => templateTaskRowHtml(template.id, task)).join("") || '<div class="empty"><span class="empty-title">还没有任务</span></div>'}
      </div>
    </div>` : "";
  return `
    <section class="group template-card ${active ? "active" : ""}" data-template-id="${template.id}">
      <button class="template-card-header" data-action="toggle-template" data-template-id="${template.id}">
        <i data-lucide="${expanded ? "chevron-down" : "chevron-right"}" class="icon-14"></i>
        ${nameHtml}
        <span class="count">${template.tasks.length} 项</span>
      </button>
      ${body}
    </section>`;
}

function renderTemplates() {
  const wrap = $("#taskLists");
  const cards = state.templates.map(templateCardHtml).join("");
  wrap.innerHTML = `
    <div class="template-toolbar">
      <button class="mini-btn" data-action="create-template-from-today"><i data-lucide="download"></i>从今天新建模板</button>
      <button class="mini-btn primary" data-action="apply-templates"><i data-lucide="copy-check"></i>全部应用到今天</button>
    </div>
    ${cards || '<div class="empty"><span class="empty-title">还没有模板</span></div>'}
  `;
}

function renderLists() {
  if (state.settings.view === "history") {
    renderHistory();
  } else if (state.settings.view === "templates") {
    renderTemplates();
  } else {
    renderToday();
  }
}

function renderDate() {
  const now = new Date();
  $("#dateText").textContent = `${now.getMonth() + 1}月${now.getDate()}日 ${WEEK[now.getDay()]}`;
  $("#dateSub").textContent = `${now.getFullYear()}年`;
}

function renderProgress() {
  const key = todayKey();
  const todayTasks = state.tasks.filter((task) => taskDate(task) === key);
  const done = todayTasks.filter((task) => task.done).length;
  const total = todayTasks.length;
  const percent = total ? Math.round((done / total) * 100) : 0;
  const circumference = 2 * Math.PI * 17;

  $("#doneCount").textContent = done;
  $("#totalCount").textContent = total;
  $("#ringText").textContent = `${percent}%`;

  const ring = $("#ringValue");
  ring.style.strokeDasharray = circumference;
  ring.style.strokeDashoffset = circumference * (1 - (total ? done / total : 0));
  document.documentElement.style.setProperty("--strip-progress", `${percent}%`);
}

function getDefaultPanelColor() {
  return state.settings.theme === "dark" ? "#202326" : "#ffffff";
}

function applyPanelStyle() {
  const opacity = Math.max(1, Math.min(100, Number(state.settings.panelOpacity) || 100));
  const color = state.settings.panelColor || getDefaultPanelColor();
  document.documentElement.style.setProperty("--panel-opacity-pct", `${opacity}%`);
  document.documentElement.style.setProperty("--panel-solid", color);
  $("#panelColorInput").value = color;
  $("#opacityRange").value = String(opacity);
  $("#opacityLabel").textContent = `${opacity}%`;
}

function buildTimeColumns() {
  const hourColumn = $("#hourColumn");
  const minuteColumn = $("#minuteColumn");
  hourColumn.innerHTML = "";
  minuteColumn.innerHTML = "";

  for (let hour = 0; hour < 24; hour += 1) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "time-item";
    button.dataset.unit = "hour";
    button.dataset.value = String(hour);
    button.textContent = String(hour).padStart(2, "0");
    hourColumn.appendChild(button);
  }

  for (let minute = 0; minute < 60; minute += 1) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "time-item";
    button.dataset.unit = "minute";
    button.dataset.value = String(minute);
    button.textContent = String(minute).padStart(2, "0");
    minuteColumn.appendChild(button);
  }

  renderTimeSelection();
}

function renderTimeSelection() {
  const { hour, minute } = state.timePicker;
  $$("#hourColumn .time-item").forEach((item) => {
    item.classList.toggle("active", Number(item.dataset.value) === hour);
  });
  $$("#minuteColumn .time-item").forEach((item) => {
    item.classList.toggle("active", Number(item.dataset.value) === minute);
  });
  scrollTimeColumns();
}

function scrollTimeColumns() {
  [
    ["#hourColumn", state.timePicker.hour],
    ["#minuteColumn", state.timePicker.minute]
  ].forEach(([selector, value]) => {
    const column = $(selector);
    const active = column.querySelector(`.time-item[data-value="${value}"]`);
    if (active) {
      column.scrollTop = active.offsetTop - (column.clientHeight - active.offsetHeight) / 2;
    }
  });
}

function toggleTimePicker() {
  const popover = $("#timePopover");
  if (!popover.hidden) {
    popover.hidden = true;
    return;
  }
  const parts = state.addTime ? state.addTime.split(":").map(Number) : [12, 0];
  state.timePicker.hour = parts[0] || 0;
  state.timePicker.minute = parts[1] || 0;
  renderTimeSelection();
  popover.hidden = false;
}

function confirmTimePicker() {
  const { hour, minute } = state.timePicker;
  state.addTime = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  $("#timePopover").hidden = true;
  const dateLabel = state.addDate === todayKey() ? "" : `${shortDateLabel(state.addDate)} `;
  $("#timeText").textContent = `${dateLabel}${state.addTime}`;
}

function clearTimePicker() {
  state.addTime = "";
  state.addDate = todayKey();
  $("#timePopover").hidden = true;
  $("#timeText").textContent = "时间";
}

function renderChrome() {
  const app = $("#app");
  app.dataset.dock = state.settings.dock;
  app.dataset.collapseDirection = state.settings.collapseDirection;
  app.classList.toggle("collapsed", state.collapsed);
  app.classList.toggle("locked", state.settings.locked);
  document.documentElement.dataset.theme = state.settings.theme;

  $$(".tab").forEach((tab) => tab.classList.toggle("active", tab.dataset.view === state.settings.view));
  $$(".chip").forEach((chip) => chip.classList.toggle("active", chip.dataset.period === state.addPeriod));
  $$(".prio-chip").forEach((chip) => chip.classList.toggle("active", Number(chip.dataset.priority) === state.addPriority));
  $$(".seg-btn[data-action='dock']").forEach((btn) => btn.classList.toggle("active", btn.dataset.dock === state.settings.dock));
  $$(".seg-btn[data-action='collapse-direction']").forEach((btn) => btn.classList.toggle("active", btn.dataset.direction === state.settings.collapseDirection));

  const isTemplateView = state.settings.view === "templates";
  $("h1").textContent = isTemplateView ? "任务模板" : "今日任务";
  $(".progress-section").style.display = isTemplateView ? "none" : "";
  if (isTemplateView) {
    const activeTemplate = state.templates.find((item) => item.id === state.activeTemplateId);
    $("#taskInput").placeholder = activeTemplate ? `添加到 ${activeTemplate.name}` : "添加模板任务";
  } else {
    $("#taskInput").placeholder = "添加今天的任务";
  }
  $("#addBtn").title = isTemplateView ? "添加到模板" : "添加任务";
  const autoToggle = $('[data-role="auto-toggle"]');
  autoToggle.classList.toggle("on", state.settings.autoCollapse);
  const autostartToggle = $('[data-role="autostart-toggle"]');
  autostartToggle.classList.toggle("on", state.settings.autostart);
  $("#autostartRow").style.display = isTauriApp ? "" : "none";
  $("#settingsPopover").hidden = !state.settingsOpen;
  $("#themeBtn").innerHTML = state.settings.theme === "dark"
    ? '<i data-lucide="sun"></i>'
    : '<i data-lucide="moon"></i>';
  $("#lockBtn").innerHTML = state.settings.locked
    ? '<i data-lucide="lock-open"></i>'
    : '<i data-lucide="lock"></i>';
  $("#lockBtn").title = state.settings.locked ? "解锁" : "锁定";
  $("#lockBtn").setAttribute("aria-label", state.settings.locked ? "解锁" : "锁定");
  const dateInput = $("#taskDateInput");
  if (dateInput) dateInput.value = state.addDate;
  const dateLabel = state.addDate === todayKey() ? "" : `${shortDateLabel(state.addDate)} `;
  $("#timeText").textContent = state.addTime
    ? `${dateLabel}${state.addTime}`
    : (dateLabel ? `${dateLabel}全天` : "时间");
  $("#timePopover").hidden = true;
  applyPanelStyle();
}

function render() {
  renderChrome();
  renderDate();
  renderProgress();
  renderLists();
  if (window.lucide) window.lucide.createIcons();
}

function setCollapsed(collapsed) {
  state.collapsed = collapsed;
  $("#app").classList.toggle("collapsed", collapsed);
  if (!collapsed) {
    expandedGraceUntil = Date.now() + 600;
  }
  if (isTauriApp) {
    invokeTauri("apply_window_state", {
      collapsed,
      dock: state.settings.dock,
      direction: state.settings.collapseDirection
    })
      .catch((error) => console.warn("调整窗口宽度失败", error));
  }
}

async function applyDesktopWindow() {
  if (!isTauriApp) return;
  try {
    await invokeTauri("apply_window_state", {
      collapsed: state.collapsed,
      dock: state.settings.dock,
      direction: state.settings.collapseDirection
    });
  } catch (error) {
    console.warn("调整窗口失败", error);
  }
}

function toggleTheme() {
  state.settings.theme = state.settings.theme === "dark" ? "light" : "dark";
  save();
  render();
}

function toggleLock() {
  state.settings.locked = !state.settings.locked;
  if (state.settings.locked && document.activeElement && typeof document.activeElement.blur === "function") {
    document.activeElement.blur();
  }
  save();
  render();
  if (isTauriApp) {
    invokeTauri("set_locked", { locked: state.settings.locked })
      .catch((error) => console.warn("设置锁定状态失败", error));
  }
}

function clearDone() {
  state.tasks = state.tasks.filter((task) => !task.done);
  save();
  render();
}

function resetSample() {
  state.tasks = [];
  state.history = [];
  state.seeded = true;
  state.templates = [];
  state.templatesSeeded = true;
  state.activeTemplateId = null;
  state.expandedTemplates.clear();
  save();
  render();
}

function commitEdit(id, value) {
  if (!state.editingId) return;
  const task = findTask(id);
  state.editingId = null;
  if (task && value.trim()) {
    task.title = value.trim();
    save();
  }
  render();
}

document.addEventListener("click", (event) => {
  const clickedActionElement = event.target.closest("[data-action]");
  const clickedAction = clickedActionElement ? clickedActionElement.dataset.action : null;
  if (state.settings.locked && clickedAction !== "lock" && clickedAction !== "expand") {
    return;
  }
  if (event.target.closest(".template-rename-input")) {
    return;
  }

  const settingsButton = event.target.closest("#settingsBtn");
  const popover = $("#settingsPopover");

  if (settingsButton) {
    state.settingsOpen = !state.settingsOpen;
    render();
    return;
  }

  if (state.settingsOpen && !popover.contains(event.target)) {
    state.settingsOpen = false;
    render();
    return;
  }

  const timePopover = $("#timePopover");
  if (!timePopover.hidden && !event.target.closest(".time-picker")) {
    timePopover.hidden = true;
  }

  const actionElement = event.target.closest("[data-action]");
  if (!actionElement) return;
  const action = actionElement.dataset.action;
  const row = event.target.closest(".task-row");
  const id = row ? row.dataset.id : null;

  if (action === "toggle" && id) toggleTask(id);
  if (action === "delete" && id) deleteTask(id);
  if (action === "drag") return;
  if (action === "collapse") setCollapsed(true);
  if (action === "expand") setCollapsed(false);
  if (action === "lock") toggleLock();
  if (action === "toggle-time-picker") toggleTimePicker();
  if (action === "confirm-time") confirmTimePicker();
  if (action === "clear-time") clearTimePicker();
  if (action === "theme") toggleTheme();
  if (action === "auto-collapse") {
    state.settings.autoCollapse = !state.settings.autoCollapse;
    save();
    render();
  }
  if (action === "collapse-direction") {
    state.settings.collapseDirection = actionElement.dataset.direction;
    save();
    render();
    if (isTauriApp && state.collapsed) {
      setCollapsed(true);
    }
  }
  if (action === "toggle-autostart") {
    const next = !state.settings.autostart;
    state.settings.autostart = next;
    save();
    render();
    if (isTauriApp) {
      invokeTauri("set_autostart", { enabled: next })
        .catch((error) => {
          state.settings.autostart = !next;
          save();
          render();
          console.warn("设置开机自启失败", error);
        });
    }
  }
  if (action === "panel-color-default") {
    state.settings.panelColor = null;
    save();
    render();
  }
  if (action === "dock") {
    state.settings.dock = actionElement.dataset.dock;
    save();
    render();
    if (isTauriApp) applyDesktopWindow();
  }
  if (action === "tab") {
    state.settings.view = actionElement.dataset.view;
    save();
    render();
  }
  if (action === "add-period") {
    state.addPeriod = actionElement.dataset.period;
    render();
  }
  if (action === "add-priority") {
    state.addPriority = Number(actionElement.dataset.priority);
    render();
  }
  if (action === "add") {
    if (state.settings.view === "templates") addTemplate();
    else addTask();
  }
  if (action === "apply-template") applyTemplates(actionElement.dataset.templateId);
  if (action === "apply-templates") applyTemplates();
  if (action === "apply-template-task") applyTemplateTask(actionElement.dataset.templateId, actionElement.dataset.taskId);
  if (action === "delete-template") deleteTemplate(actionElement.dataset.templateId);
  if (action === "delete-template-task") deleteTemplateTask(actionElement.dataset.templateId, actionElement.dataset.taskId);
  if (action === "update-template-from-today") updateTemplateFromToday(actionElement.dataset.templateId);
  if (action === "create-template-from-today") createTemplateFromToday();
  if (action === "create-from-today") createTemplateFromToday();
  if (action === "toggle-template") {
    clearTimeout(templateToggleTimer);
    templateToggleTimer = setTimeout(() => {
      toggleTemplate(actionElement.dataset.templateId);
    }, 220);
  }
  if (action === "toggle-history-date") toggleHistoryDate(actionElement.dataset.date);
  if (action === "rename-template") startRenameTemplate(actionElement.dataset.templateId);
  if (action === "clear-done") {
    state.settingsOpen = false;
    clearDone();
  }
  if (action === "reset") {
    state.settingsOpen = false;
    resetSample();
  }
});

document.addEventListener("dblclick", (event) => {
  if (event.target.closest(".history-row")) {
    return;
  }

  const templateName = event.target.closest(".template-name");
  if (templateName) {
    const templateCard = templateName.closest(".template-card");
    if (templateCard) {
      clearTimeout(templateToggleTimer);
      startRenameTemplate(templateCard.dataset.templateId);
    }
    return;
  }

  const titleElement = event.target.closest(".task-title");
  if (!titleElement) return;
  const row = titleElement.closest(".task-row");
  if (!row) return;
  state.editingId = row.dataset.id;
  render();
  requestAnimationFrame(() => {
    const input = $(".edit-input");
    if (input) {
      input.focus();
      input.select();
    }
  });
});

document.addEventListener("focusout", (event) => {
  if (event.target.classList.contains("edit-input")) {
    commitEdit(event.target.dataset.editId, event.target.value);
  }
  if (event.target.classList.contains("template-rename-input")) {
    commitTemplateRename(event.target.dataset.renameId, event.target.value);
  }
});

document.addEventListener("keydown", (event) => {
  if (event.target.classList.contains("edit-input")) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitEdit(event.target.dataset.editId, event.target.value);
    } else if (event.key === "Escape") {
      state.editingId = null;
      render();
    }
  }
  if (event.target.classList.contains("template-rename-input")) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitTemplateRename(event.target.dataset.renameId, event.target.value);
    } else if (event.key === "Escape") {
      cancelTemplateRename();
    }
  }
});

$("#taskInput").addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    if (state.settings.view === "templates") addTemplate();
    else addTask();
  }
});

$("#panelColorInput").addEventListener("input", (event) => {
  state.settings.panelColor = event.target.value;
  save();
  applyPanelStyle();
});

$("#opacityRange").addEventListener("input", (event) => {
  state.settings.panelOpacity = Number(event.target.value);
  save();
  applyPanelStyle();
});

$("#taskDateInput").addEventListener("change", (event) => {
  state.addDate = event.target.value || todayKey();
  const dateLabel = state.addDate === todayKey() ? "" : `${shortDateLabel(state.addDate)} `;
  $("#timeText").textContent = state.addTime
    ? `${dateLabel}${state.addTime}`
    : (dateLabel ? `${dateLabel}全天` : "时间");
});

$("#hourColumn").addEventListener("click", (event) => {
  const item = event.target.closest(".time-item");
  if (!item) return;
  state.timePicker.hour = Number(item.dataset.value);
  renderTimeSelection();
});

$("#minuteColumn").addEventListener("click", (event) => {
  const item = event.target.closest(".time-item");
  if (!item) return;
  state.timePicker.minute = Number(item.dataset.value);
  renderTimeSelection();
});

function isDroppablePeriod(period) {
  return PERIODS.some((item) => item.id === period);
}

function getDropIndex(list, clientY) {
  const rows = Array.from(list.querySelectorAll(".task-row"))
    .filter((row) => row.dataset.id !== dragId);
  if (!rows.length) return 0;
  for (let i = 0; i < rows.length; i++) {
    const rect = rows[i].getBoundingClientRect();
    if (clientY < rect.top + rect.height / 2) return i;
  }
  return rows.length;
}

function clearDropIndicators() {
  $$(".drop-indicator").forEach((element) => element.remove());
  $$(".task-list.drop-list-empty").forEach((element) => element.classList.remove("drop-list-empty"));
}

function showDropIndicator(list, index) {
  const rows = Array.from(list.querySelectorAll(".task-row"))
    .filter((row) => row.dataset.id !== dragId);
  if (!rows.length) {
    list.classList.add("drop-list-empty");
    return;
  }
  const indicator = document.createElement("div");
  indicator.className = "drop-indicator";
  const anchor = rows[index];
  if (anchor) list.insertBefore(indicator, anchor);
  else list.appendChild(indicator);
}

document.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || state.settings.locked || state.settings.view !== "today") return;
  const row = event.target.closest(".task-row");
  if (!row || row.classList.contains("editing")) return;
  const button = event.target.closest("button");
  if (button && !button.classList.contains("grip")) return;
  pointerDragActive = true;
  dragId = row.dataset.id;
  dropTarget = null;
  row.classList.add("dragging");
  event.preventDefault();
});

document.addEventListener("pointermove", (event) => {
  if (!pointerDragActive || !dragId) return;
  event.preventDefault();
  const lists = $$(".task-list").filter((list) => {
    const rect = list.getBoundingClientRect();
    return event.clientY >= rect.top && event.clientY <= rect.bottom;
  });
  const list = lists[0];
  if (!list || !isDroppablePeriod(list.dataset.period)) {
    clearDropIndicators();
    dropTarget = null;
    return;
  }
  const index = getDropIndex(list, event.clientY);
  dropTarget = { period: list.dataset.period, index };
  clearDropIndicators();
  showDropIndicator(list, index);
});

function finishPointerDrag(event) {
  if (!pointerDragActive) return;
  if (event && event.type === "pointerup" && dragId && dropTarget) {
    moveToIndex(dragId, dropTarget.period, dropTarget.index);
  }
  pointerDragActive = false;
  dragId = null;
  dropTarget = null;
  clearDropIndicators();
  $$(".task-row.dragging").forEach((element) => element.classList.remove("dragging"));
}

document.addEventListener("pointerup", finishPointerDrag);
document.addEventListener("pointercancel", finishPointerDrag);

const app = $("#app");
app.addEventListener("mouseenter", () => {
  clearTimeout(collapseTimer);
  expandedGraceUntil = 0;
});

app.addEventListener("mouseleave", () => {
  if (state.settings.autoCollapse && !state.settingsOpen && Date.now() > expandedGraceUntil) {
    collapseTimer = setTimeout(() => setCollapsed(true), 100);
  }
});

let lastNotifiedKey = "";

function checkTaskReminders() {
  if (!isTauriApp) return;
  const now = new Date();
  const key = todayKey(now);
  const currentTime = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
  const due = state.tasks.find(
    (task) => !task.done
      && task.time === currentTime
      && taskDate(task) === key
  );
  if (!due) return;
  const notifyKey = `${key}-${due.id}-${currentTime}`;
  if (notifyKey === lastNotifiedKey) return;
  lastNotifiedKey = notifyKey;
  invokeTauri("notify", { title: "任务提醒", body: due.title })
    .catch((error) => console.warn("发送通知失败", error));
}

if (isTauriApp) {
  document.body.classList.add("tauri");
  checkTaskReminders();
  setInterval(checkTaskReminders, 30000);
  setInterval(() => {
    if (archivePastTasks()) {
      save();
      render();
    }
  }, 60000);
}

buildTimeColumns();
load();
