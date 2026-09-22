# MicaSpeak React/Vite 设计原型

这是 MicaSpeak 的 React + TypeScript + Vite 视觉基线，最终运行在 Tauri v2 的 WebViewWindow 中。它不是独立的后端或协议客户端。

## 开发服务器

本目录可用 `pnpm dev` 启动 Vite 预览；端口以环境变量或 Vite 输出为准。预览只证明 DOM/CSS 交互，不证明 Tauri IPC、Evergreen WebView2、TS3 连接或音频。

## 结构

- `src/main.tsx`：React 入口。
- `src/App.tsx`：七种演示状态的路由/状态切换。
- `src/frames/`：连接、主窗口、设置、悬浮、断线画面。
- `src/index.css`：全局字体和主题令牌。
- `src/icons.tsx`、`src/lib.tsx`：图标与可复用视觉组件。
- `package.json`、`pnpm-lock.yaml`、`vite.config.ts`：锁定的前端构建工具链。

## 依赖和风格

- Runtime：React 19、React DOM 19。
- 构建：Vite、TypeScript、`@vitejs/plugin-react`、Tailwind CSS v4（如 manifest 已启用）。
- 设计：Segoe UI/微软雅黑回退、Fluent 线性图标、420×640 主窗、520×640 设置窗、260×80 overlay；浅深主题和状态色集中管理。
- 组件必须覆盖 loading、error、empty、disabled、speaking 和 disconnected 状态，并保持固定控件尺寸。

## Tauri 接入边界

正式前端通过 `@tauri-apps/api/core` 的 `invoke` 调 Rust command，通过 `@tauri-apps/api/event` 订阅事件。不要在这里直接读取文件、访问 UDP、操作 cpal、保存身份或假装连接成功。

M1 起将演示假数据替换为 `get_app_snapshot` 和 `app://snapshot`、`connection://state`、`channel://tree`、`chat://message`、`voice://talking`、`overlay://state` 等事件。每个 effect 必须在卸载时取消 listener，并在窗口重新加载后重新请求 snapshot。

## 验证

- `pnpm build`：只代表前端静态构建通过。
- Tauri dev：额外记录 WebView2 是否为 Evergreen、IPC 是否返回和窗口是否可见。
- 真实 TS3/音频/overlay 验收必须在任务卡中完成，不能用预览截图替代。
