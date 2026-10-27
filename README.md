<div align="center">

<img src="assets/banner.svg" alt="DeskPet — 桌面 AI 宠物" width="640" />

**桌面上养一只 AI 小猫：点击它，就能和任何大模型聊天。**

[![Version](https://img.shields.io/badge/version-0.2.4-3D7FE0?style=flat-square)](#版本历史)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4?style=flat-square&logo=windows)](#快速开始)
[![Tauri](https://img.shields.io/badge/Tauri-v2-FFC131?style=flat-square)](https://v2.tauri.app)
[![License: MIT](https://img.shields.io/badge/license-MIT-34B37F?style=flat-square)](LICENSE)
[![Size](https://img.shields.io/badge/安装包-13.3_MB-93A3B5?style=flat-square)](#快速开始)

透明置顶 · 托盘常驻 · 多模型接入 · 密钥零泄露 · 数据全程本地

[快速开始](#快速开始) · [接入模型](#接入模型) · [隐私设计](#隐私与安全设计) · [从源码构建](#从源码构建) · [Roadmap](#roadmap)

</div>

---

DeskPet 是一款基于 **Tauri v2** 的 Windows 桌面宠物应用：一只透明置顶、不入任务栏的小猫常驻桌面。**单击它**弹出微型气泡对话框，接入任意大模型流式聊天；**右键它**切换皮肤、人格与新会话；**拖动它**随意摆放（位置自动记忆）。内置长期记忆、自然语言提醒、人格卡片与主动搭话，依据完整需求规格说明书（SRS）开发，设计文档与浏览器模拟版原型见 [`docs/`](docs)。

> 🐱 开箱即用：内置**演示模式**，不配任何 API Key 也能体验完整交互链路。

## ✨ 特性一览

| | |
|---|---|
| 🪟 **真实桌面窗口** | 透明无边框、系统级置顶、不占任务栏；点击穿透一键把鼠标事件还给下层窗口 |
| 💬 **微型气泡对话** | 智能上下翻转 + 尾巴箭头定位；流式逐 token 渲染、Markdown / 代码块 / 一键复制；Esc / 外点 / 失焦 / 闲置自动关闭，关闭后恢复原前台窗口 |
| 🤖 **多模型接入** | 12+ 供应商模板：OpenAI、DeepSeek、Claude、Gemini、Kimi、智谱 GLM、通义千问、Azure、Ollama、LM Studio、vLLM、自定义兼容端点；OpenAI / Anthropic / Gemini 三协议原生适配，主进程直连（无 CORS 限制） |
| 🔐 **密钥零泄露** | API Key 存 **Windows 凭据管理器**，不进入数据文件、日志与导出内容；本地服务（Ollama 等）免 Key 直连 |
| 🧠 **长期记忆** | 对它说「记住：我喜欢蓝色」即写入记忆库，之后每轮对话按相关度自动注入（≤5 条） |
| ⏰ **自然语言提醒** | 「25 分钟后提醒我喝水」「每天 8:30 提醒我站会」→ 主进程调度器准点触发，支持稍后 5 分钟与免打扰排队补发 |
| 😼 **人格与皮肤** | 3 套内置人格（贴心小助手 / 毒舌吐槽役 / 专注工作搭子）+ 3 套皮肤联动切换；支持人格导入导出 |
| 🎭 **八态动画** | idle / drag / click / talk / sleep / work / happy / error 全矢量状态机，50%–200% 无级缩放 |
| 🖥 **系统集成** | 托盘菜单（左键唤起气泡）、全局快捷键、开机自启、会话检索与 Markdown 导出 |
| 🪶 **轻量** | 安装包 13.3 MB · 运行内存约 31 MB · 前端零构建（无 npm 依赖） |

## 🚀 快速开始

无需构建，下载即用：

1. 打开本仓库 [`dist/`](dist) 目录，下载两个文件：
   - `DeskPet.exe`（13.3 MB，主程序）
   - `WebView2Loader.dll`（0.15 MB，引导库）
2. 将两个文件放入**同一文件夹**，双击 `DeskPet.exe`

> 环境：Windows 10+ 与 WebView2 运行时（Win10/11 一般已内置）。首次运行如遇 SmartScreen 提示，选择「仍要运行」（未做代码签名）。

启动后 30 秒上手：

| 操作 | 效果 |
|---|---|
| **单击小猫** | 弹出气泡对话框，直接输入消息（默认演示模式） |
| **右键小猫** | 皮肤切换 / 新会话 / 设置 / 免打扰 / 复位位置 |
| **拖动小猫** | 随意摆放，位置自动记忆、重启恢复 |
| **托盘左键** | 唤起 / 收起气泡 |
| **托盘右键** | 完整菜单：设置、开机自启、点击穿透、退出 |
| **`Ctrl+,`** | 打开设置（`Ctrl+Shift+Space` 全局唤起气泡，若被其他程序占用则自动降级） |

## 🤖 接入模型

托盘或宠物右键 → **设置 → 模型** → 「＋ 新增供应商」：

1. **选择供应商模板**（以 Ollama 为例）
2. **填写信息**：BaseURL + 模型名 + API Key（本地服务无需 Key）
3. **测试连接** → 返回延迟与可用模型列表，可在模型名输入框下拉选择
4. 勾选 **「设为默认」** → 保存（绑定演示模式的人格会自动切换过来）

实测本地 Ollama + `llama3.2:3b`：

```text
✓ 连接成功 · 延迟 96ms · 16 个可用模型（本地服务，无需 Key）
```

**支持的接入方式：**

| 类型 | 供应商 |
|---|---|
| 云端（需 Key） | OpenAI · DeepSeek · Anthropic Claude · Google Gemini · Moonshot Kimi · 智谱 GLM · 通义千问 · Azure OpenAI |
| 本地（免 Key） | Ollama · LM Studio · vLLM |
| 自定义 | 任意 OpenAI 兼容端点（自定义 BaseURL / 请求头） |

> 💡 模型请求由 Rust 主进程发出：不受浏览器 CORS 限制，Ollama 无需设置 `OLLAMA_ORIGINS`；密钥仅存于 Windows 凭据管理器（凭据名 `DeskPet/<供应商ID>`）。

## 🛡️ 隐私与安全设计

| 设计 | 说明 |
|---|---|
| 🔑 密钥隔离 | API Key 写入系统凭据管理器，渲染层（WebView）全程不可见；导出配置与日志自动剔除 |
| 🏠 数据本地 | 会话 / 记忆 / 配置持久化于 `%APPDATA%\com.mrtransition.deskpet\`，JSON 明文可随时审查 |
| 🚫 零上报 | 无遥测、无埋点、无崩溃收集；唯一的网络请求是你自己配置的模型 API |
| 🔎 可审计 | 全部源码不到 200 KB（Rust 主进程 + 3 个零构建 HTML），一屏读完所有数据流 |

## 🏗️ 架构

```text
┌───────────────────────────── DeskPet.exe（Rust 主进程）─────────────────────────────┐
│  窗口管理（透明置顶/托盘/快捷键）  ·  密钥存取（Windows 凭据管理器）                  │
│  流式对话（OpenAI/Anthropic/Gemini 协议，reqwest + SSE）  ·  提醒调度器（后台线程）  │
└──────────┬──────────────────────┬──────────────────────┬───────────────────────────┘
           ▼ WebView2             ▼ WebView2             ▼ WebView2
     pet.html（宠物）        bubble.html（气泡）     settings.html（设置）
     矢量动画状态机           流式渲染/记忆注入        模型/人格/数据管理
```

- **前端**：`src/` 下三个零构建静态页 + 共享资源，由 `frontendDist` 嵌入 exe，无任何 npm 依赖
- **后端**：`src-tauri/src/main.rs`，窗口、托盘、快捷键、密钥、三协议流式对话与提醒调度全部收敛于主进程

## 🔨 从源码构建

工具链：Rust（`x86_64-pc-windows-gnu` 目标）+ **winlibs GCC**（binutils ≥ 2.41）。

> ⚠️ 不可使用 TDM-GCC 10.3（binutils 2.36）：其 PE 链接器存在长名 section 布局 bug，产物为非法 PE。

```powershell
# 1) 安装 Rust 并添加 GNU 目标
rustup target add x86_64-pc-windows-gnu

# 2) 下载 winlibs（https://github.com/brechtsanders/winlibs_mingw/releases）

# 3) 复制 .cargo/config.example.toml 为 .cargo/config.toml，
#    将其中 linker / ar 路径改为你本机 winlibs 的实际路径

# 4) 构建
cargo build --release --target x86_64-pc-windows-gnu --manifest-path src-tauri/Cargo.toml
# 产物：src-tauri/target/x86_64-pc-windows-gnu/release/deskpet.exe
#      （与 WebView2Loader.dll 同目录分发）
```

中国大陆网络可在 `config.toml` 中启用 rsproxy 镜像加速（模板已内置）。

## 🗺️ Roadmap

- [ ] 动画皮肤包（APNG / GIF / 帧序列导入）
- [ ] SQLite 存储与会话全文检索增强
- [ ] 自动更新（GitHub Releases 通道）
- [ ] 多语言（i18n）
- [ ] macOS / Linux 支持（依赖透明窗口与托盘的跨平台适配）
- [ ] 久坐提醒、剪贴板感知等 P2 特性（见 SRS）

## 📋 版本历史

<details open>
<summary><b>v0.2.4</b> — 修复整机冻结死锁（提醒到期后点设置卡死）</summary>

- **根因**：提醒调度线程在持有全局状态锁时调用写盘函数，而写盘函数内部会对同一把锁再次加锁——`std::sync::Mutex` 不支持同线程重入，调度线程自死锁并**永久持有状态锁**；此后任何需要该锁的命令（如打开设置时的状态加载）都会把主线程永久阻塞，导致整个应用冻结。仅在存在已到期的一次性提醒时触发，故此前的干净环境测试未暴露。
- **修复**：写盘函数重构为绝不接触状态锁（改为接收序列化快照）；持锁场景改为锁内序列化、锁外写盘。新增回归单元测试：持锁调用写盘必须立即完成（若回退将挂起测试）。
- 附带：设置页「关于」版本号改为从 `tauri.conf.json` 读取（此前硬编码已与实际版本脱节）；修正一处与 http 头校验规则不符的测试断言（控制字符非法，空格与 obs-text 字节合法）。
</details>

<details>
<summary><b>v0.2.3</b> — 新增供应商按钮反馈修复</summary>

修复「＋ 新增供应商」点击无反馈：点击后弹出模式提示、名称框聚焦全选；修复模板下拉与表单内容不一致及编辑态切入时名称残留问题。
</details>

<details>
<summary><b>v0.2.2</b> — 崩溃与 Ollama 直连修复</summary>

- **崩溃**：API Key 含非法字符时主进程 panic 导致进程静默消失（release 为 `panic="abort"`）。改为返回明确错误，并全量加固状态锁（锁中毒防护）
- **Ollama 调用不通**：测试连接强制要求 Key，本地服务被拦截。现本地地址免 Key 直连，云端无 Key 由服务端 401 反馈
- **演示模式不切换**：「设为默认」保存时自动迁移仍绑演示模式的人格与会话
</details>

<details>
<summary><b>v0.2.1</b> — 设置窗口白屏修复</summary>

设置窗口由懒创建改为启动时预创建隐藏（与气泡窗口同模式），首次打开即刻显示已加载内容，消除 WebView2 冷启动白屏。
</details>

<details>
<summary><b>v0.2.0</b> — 桌面版首发</summary>

Tauri v2 桌面壳：透明置顶宠物窗口、气泡对话、托盘、全局快捷键、开机自启、密钥入系统凭据管理器、主进程三协议流式对话、人格 / 记忆 / 提醒 / 主动搭话、会话管理与导出。
</details>

<details>
<summary><b>v0.1.0</b> — 浏览器模拟版（MVP）</summary>

单文件零依赖 HTML 原型，验证全部交互与数据链路，保留于 [`docs/MVP-浏览器模拟版.html`](docs/MVP-浏览器模拟版.html)。
</details>

完整需求规格（SRS-DESKPET-AI-001 v1.0）与需求追溯矩阵见 [`docs/`](docs)。

## 🤝 贡献

欢迎 Issue 与 PR：功能建议、Bug 反馈（附触发路径与 `%APPDATA%\com.mrtransition.deskpet\deskpet.log` 日志）、文档改进均受欢迎。

## 📄 许可

[MIT License](LICENSE) © 2026 mrtransition

> 本项目由 AI 协作开发，依据完整需求规格说明书迭代实现。

> AI生成
