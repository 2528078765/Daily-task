# 每日任务侧边栏

一个贴边的每日任务桌面工具，支持任务分组、模板、历史待办、锁定穿透、收起方向、置底显示。

## 运行

Web 原型可以直接打开根目录的 `index.html`。

桌面版需要 Node.js 和 Rust 环境：

```bash
npm install
npm run desktop:dist
npm run tauri dev
```

## 打包

```bash
npm run desktop:dist
npm run tauri build
```

## 数据

桌面版数据保存在 JSON 文件中：

```text
%APPDATA%\com.daily-task-sidebar.app\data.json
```
