# 每日任务侧边栏

一个固定在屏幕右侧的每日任务桌面工具，支持任务分组、模板、历史待办和壁纸级锁定。

## 功能

- 右侧 320px 侧边栏，固定上下拉满，不会遮挡任务栏
- 任务按全天、上午、下午、晚上分组
- 支持为任务设置具体时间，到点弹出系统通知
- 支持设置未来待办，灰色显示且暂不能完成，到当天后自动恢复
- 过期待办自动进入历史待办，不再显示在今日列表
- 完成任务后保持当前位置，不会沉底
- 拖拽调整任务顺序
- 模板：模板 1、模板 2 可展开、重命名，支持从今天创建、应用到今天、单条添加
- 历史待办：按日期只读展示，标题为当天日期，可直接修改 JSON 文件调整
- 锁定：锁定后位于桌面图标下面、壁纸上面，点击全部穿透，不会被显示桌面或最小化隐藏，只能通过托盘右键菜单解锁
- 系统托盘：右键可解锁或退出
- 设置：主题、开机自启、面板纯色、不透明度 1%-100%
- 数据使用 JSON 存储，自动备份

## 界面预览

![界面预览 1](预览图1.png)

![界面预览 2](预览图2.png)

![界面预览 3](预览图3.png)

## 下载使用

### 方式一：直接安装

从 [Releases](https://github.com/2528078765/Daily-task/releases/latest) 下载 `daily-task-sidebar-0.1.0-setup.exe`，双击安装后即可使用。

安装后首次启动没有任何示例任务，任务、历史、模板都是空的，需要自己添加。

数据保存在：

```text
%APPDATA%\com.daily-task-sidebar.app\data.json
```

每次保存前会自动备份为同目录下的 `data.backup.json`。

### 方式二：从源码运行桌面版

环境要求：

- Windows 10/11
- Node.js 18 或更高
- Rust stable 工具链
- WebView2 运行时（Windows 10/11 一般已内置）

```bash
npm install
npm run desktop:dist
npm run tauri dev
```

`npm run desktop:dist` 会把根目录的 `index.html`、`styles.css`、`app.js` 同步到 `dist/`，桌面版使用 `dist/` 作为前端资源。

### 打包安装程序

```bash
npm run desktop:dist
npm run tauri build
```

打包产物在：

```text
src-tauri/target/release/bundle/nsis/
```

如果下载 NSIS 打包工具较慢，可以设置 GitHub 镜像：

```powershell
$env:TAURI_BUNDLER_TOOLS_GITHUB_MIRROR_TEMPLATE = "https://gh-proxy.com/https://github.com/<owner>/<repo>/releases/download/<version>/<asset>"
npm run tauri build
```

## 使用说明

- 添加任务：在底部输入框输入标题，也可以先选择时段、优先级和时间再添加。
- 完成任务：点击任务左侧的勾选圆点，任务会保留在原位置。
- 调整顺序：按住任务拖动到目标位置。
- 模板：进入“模板”页，创建模板 1、模板 2，把今天的任务保存为模板，或者把模板应用到今天。
- 锁定：点击标题栏锁图标，窗口会进入桌面图标下面的壁纸层并变为点击穿透；只有系统托盘图标右键菜单里的“解锁”可以恢复。
- 托盘：系统托盘中的图标右键可以解锁或退出。

## 项目结构

```text
index.html           桌面版前端页面
styles.css           样式
app.js               前端逻辑
dist/                桌面版使用的前端资源
src-tauri/           Tauri 桌面端源码
vendor/              前端依赖资源
sync-dist.ps1        同步 dist 的脚本
```
