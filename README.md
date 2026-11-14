# 浮槎 — the Star Raft

> 天河与海通，年年八月有浮槎去来，不失期。——张华《博物志》

用 Tauri 2 + React 19 + shadcn/ui 做的 [Mister Download Manager](https://github.com/anefzaoui/misterdownloadmanager) 前端。

浮槎只负责界面：下载、分段、断点续传、浏览器接管和种子都由本机的
Mister Download Manager 完成。浮槎内置一个只监听 `127.0.0.1:8484` 的
axum 服务：前端通过 HTTP 调它，它再用 MDM 的一行 JSON 协议调 MDM。

「浮槎」是每年八月准时往返于大海与天河之间的木筏，从不误期。
Slogan：**年年八月，不失期。** / *Every August, without fail.*

## 环境

- Windows 10/11（macOS / Linux 理论可用，未测）
- Node.js 22+
- Rust 1.95+
- Mister Download Manager v0.5.2+

## 运行

```bash
cd fucha
npm install
```

桌面应用（axum 由 Tauri 在启动时拉起）：

```bash
npm run tauri dev
```

浏览器开发（Vite + 独立的 axum 进程）：

```bash
npm run dev
```

`npm run dev` 会同时启动 Vite（1420）和 `fucha-server`（`127.0.0.1:8484`）。
浏览器里的 `/api` 请求由 Vite 代理到 axum；打包后前端直接访问同一端口。

也可以分开跑：

```bash
npm run dev:web
```

```bash
npm run api
```

第一次使用前，启动 MDM 并打开自动化 API（只需一次）：

```bash
misterdownloadmanager-cli api on
```

MDM 每次启动会把端口和 token 写进
`%APPDATA%\misterdownloadmanager\ipc.json`，浮槎自动读取。
便携版或自定义数据目录可以用环境变量 `MDM_IPC_PATH` 指向对应文件。

### 没有 MDM 也能看界面

打开 `http://localhost:1420/?mock=1`，前端会使用内置的模拟数据
（`src/lib/mdm/mock.ts`），列表会自动跑进度，可以完整点一遍交互。
不加参数时会走真实的本地 API。`?offline=1` 可以预览「未连接 MDM」的引导页。

## 设置

侧栏底部的齿轮打开设置：

- **默认下载目录**：写入 MDM 的 `General` 分类目录（`downloads.json` 的
  `settings.category_dirs`），其他分类没有单独设置时都用它。
- **MDM 运行方式**：
  - **后台引擎（推荐）**：以 `--headless --downloads <目录>` 运行。MDM 不显示任何窗口，
    也没有经典下载弹窗，添加/进度全部在浮槎里。
  - **MDM 窗口**：保留 MDM 自己的主窗口、设置和下载弹窗，在托盘运行。

保存会重启 MDM，进行中的下载会中断，之后可以继续。MDM 的安装路径默认从
`%LOCALAPPDATA%\Programs\Mister Download Manager` 查找，其他位置可以用环境变量
`MDM_EXE` 指定。

## 打包

```bash
npm run tauri build
```

不生成安装包、只要可执行文件：

```bash
npm run tauri build -- --no-bundle
```

产物：`src-tauri/target/release/fucha.exe`。

## 结构

| 路径 | 作用 |
|---|---|
| `src-tauri/src/api.rs` | axum 路由：`/api/mdm` 透传 MDM 协议，另有设置、选目录、打开链接等本机操作 |
| `src-tauri/src/mdm.rs` | 读写 MDM 的 `ipc.json`、走本地 TCP 调自动化 API；设置页的启动/重启逻辑 |
| `src-tauri/src/bin/fucha-server.rs` | 浏览器开发用的独立 API 进程 |
| `src/lib/mdm/client.ts` | 前端传输层：统一 `fetch` 本地 API，`?mock=1` 时走模拟数据 |
| `src/hooks/use-mdm.ts` | 每秒轮询下载列表 |
| `src/App.tsx` | 布局、筛选、批量操作 |
| `src/components/` | 品牌标识、侧栏、下载表格、统计卡片、添加/设置对话框 |

本地 API 只监听 `127.0.0.1:8484`：

| 方法 | 路径 | 作用 |
|---|---|---|
| `GET` | `/api/health` | 存活检查 |
| `POST` | `/api/mdm` | 把 JSON 原样发给 MDM（单行协议），返回 MDM 的回复 |
| `GET` | `/api/ipc-path` | 当前使用的 `ipc.json` 路径 |
| `GET` | `/api/download-dir` | MDM 的 General 分类下载目录 |
| `GET` | `/api/system-download-dir` | 系统默认下载目录 |
| `POST` | `/api/settings` | 保存下载目录和运行方式，重启 MDM |
| `POST` | `/api/pick-directory` | 打开原生文件夹选择器 |
| `POST` | `/api/open` | 用默认浏览器打开 http/https 链接 |
| `POST` | `/api/reveal` | 在资源管理器中定位文件 |

例如：

```bash
curl -X POST http://127.0.0.1:8484/api/mdm -H "content-type: application/json" -d "{\"query\":\"list\"}"
```

## 当前边界

自动化 API 覆盖下载列表、添加、暂停/继续、移除/删除、队列和分类，**不覆盖**设置页、
种子文件勾选和重复文件确认。后台模式下这些操作没有 MDM 窗口可用，需要时可在设置里切回
「MDM 窗口」；MDM 自己的进度窗口和对话框是它用 egui 画的，浮槎无法改变它们的样式，
只能通过后台模式让它们不出现。

## 许可

浮槎的代码以 **GPL-3.0-or-later** 发布，见 [LICENSE](LICENSE)。

Mister Download Manager 是独立的 GPL-3.0 项目，见
[它的仓库](https://github.com/anefzaoui/misterdownloadmanager)；本项目不包含 MDM 的代码或图标。
