# 每日任务侧边栏

一个贴边的每日任务桌面工具，支持任务分组、模板、历史待办、锁定穿透、收起方向和桌面置底显示。

## 功能

- 右侧侧边栏，可贴右上缘或右侧上下拉满
- 收起方向可选：往右收成右侧细条，或往上收成顶部横条
- 鼠标离开后自动收起，点击细条展开
- 任务按全天、上午、下午、晚上分组
- 支持设置具体时间，到点弹出系统通知
- 完成任务后保持当前位置，不会沉底
- 拖拽调整任务顺序
- 模板：模板 1、模板 2 可展开、重命名，支持从今天创建、应用到今天、单条添加
- 历史待办：按日期只读展示，标题为当天日期，可直接修改 JSON 文件调整
- 锁定：锁定后鼠标点击穿透到桌面，右上角区域可点击解锁
- 桌面置底：不会遮挡 PyCharm 等普通窗口
- 系统托盘：左键显示/隐藏，右键可退出
- 纯色背景和不透明度 1%-100% 调节
- 数据使用 JSON 存储，自动备份

## 下载使用

### 方式一：直接安装

仓库根目录的 `每日任务侧边栏-安装包.exe` 就是 Windows 安装包，双击安装后即可使用。

安装后首次启动没有任何示例任务，任务、历史、模板都是空的，需要自己添加。

数据保存在：

```text
%APPDATA%\com.daily-task-sidebar.app\data.json
```

每次保存前会自动备份为同目录下的 `data.backup.json`。

### 方式二：Web 原型

直接打开项目根目录的 `index.html` 即可在浏览器里使用，数据会保存在浏览器 `localStorage` 中。

### 方式三：从源码运行桌面版

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

- 添加任务：在底部输入框输入标题，输入 `HH:MM` 可自动识别时间，也可以先选择时段、优先级和时间再添加。
- 完成任务：点击任务左侧的勾选圆点，任务会保留在原位置。
- 调整顺序：按住任务拖动到目标位置。
- 模板：进入“模板”页，创建模板 1、模板 2，把今天的任务保存为模板，或者把模板应用到今天。
- 锁定：点击标题栏锁图标，窗口会变为鼠标穿透，可以操作背后的桌面图标；点右上角区域可解锁。
- 收起：点击标题栏收起按钮，或在设置里选择“往右收”/“往上收”，鼠标离开后会自动收起。
- 托盘：系统托盘中的图标左键显示/隐藏窗口，右键菜单可以退出。

## 项目结构

```text
index.html           Web 原型页面
styles.css           样式
app.js               前端逻辑
dist/                桌面版使用的前端资源
src-tauri/           Tauri 桌面端源码
sync-dist.ps1        同步 dist 的脚本
```
