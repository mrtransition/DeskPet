# 桌面 AI 宠物 DeskPet

基于 Tauri v2 的 Windows 桌面宠物：点击宠物弹出微型气泡对话，可接入 OpenAI / DeepSeek / Claude / Gemini / Ollama 等任意模型，内置长期记忆、自然语言提醒、人格与皮肤。依据需求规格说明书 SRS-DESKPET-AI-001 v1.0 开发（见 `docs/`）。

## 快速上手（免构建）

直接从仓库下载成品：

```
dist/
├── DeskPet.exe          13.4 MB（主程序）
└── WebView2Loader.dll   0.15 MB（WebView2 引导库，必须与 exe 同目录）
```

下载 `dist/` 目录下两个文件放到同一文件夹，双击 `DeskPet.exe` 即可（需 Windows 10+ 与 WebView2 运行时，一般已内置）。启动后：
- 宠物（蓝色小猫「团子」）出现在屏幕右下角，透明无边框、置顶、不占任务栏
- **单击宠物** → 弹出气泡对话框，直接对话（默认演示模式，无需 API Key）
- **右键宠物** → 皮肤切换 / 新会话 / 设置 / 免打扰 / 复位位置
- **拖动宠物** 可移动位置（松手自动记忆，重启恢复）
- **托盘图标**（任务栏右下角）：左键唤起气泡，右键完整菜单（含开机自启、点击穿透、退出）

## 接入真实模型

托盘或宠物右键 → 设置 → 模型：
1. 选择供应商模板（DeepSeek / OpenAI / Claude / Gemini / Ollama / Kimi / 智谱 / 千问 / LM Studio / vLLM / 自定义）
2. 填写 BaseURL 与 API Key（本地 Ollama 等无需 Key）→ **测试连接**（返回延迟与可用模型列表）
3. 勾选「设为默认」保存（绑演示模式的人格会自动切换过来）

> 密钥存入 **Windows 凭据管理器**（凭据名 `DeskPet/<供应商ID>`），不进入数据文件、日志与导出内容（FR-MDL-007 / NFR-SEC-001 / DS-001）。
> 模型请求由主进程发出：无浏览器 CORS 限制，Ollama 本地服务无需设置 `OLLAMA_ORIGINS`（CON-07 / FR-MDL-013）。

## 核心能力对照（SRS）

| 能力 | 状态 |
|---|---|
| 透明置顶宠物窗口 / 不入任务栏（FR-PET-001~003） | ✅ 真实系统窗口 |
| 拖拽 + 位置持久化 + 越界回收（FR-PET-004/005） | ✅ |
| 八态动画状态机（idle/drag/click/talk/sleep/work/happy/error，FR-PET-006） | ✅ |
| 50%–200% 缩放（FR-PET-007） | ✅ 设置 → 宠物与交互 |
| 点击穿透 + 托盘恢复（FR-PET-008） | ✅ |
| 气泡：智能上下翻转 / 尾巴箭头 / Esc·外点·失焦·闲置关闭 / 前台恢复（FR-BUB-001~006/013, NFR-INT-001） | ✅ |
| 流式逐 token 输出 + 停止生成 + 非流式自动降级（FR-BUB-008/014, FR-MDL-011） | ✅ 主进程 SSE |
| OpenAI / Anthropic / Gemini 三协议适配（NFR-INT-004） | ✅ |
| 多供应商 / 连通测试 / 模型列表 / 快速切换（FR-MDL-001~006） | ✅ |
| 密钥安全存储 + 主进程请求（FR-MDL-007/013） | ✅ Windows 凭据管理器 |
| 人格卡片 / 3 套内置人格 / 皮肤联动 / 导入导出（FR-PSN） | ✅ |
| 长期记忆「记住：…」+ 相关度注入 ≤5 条（FR-MEM-001/002/004） | ✅ |
| 自然语言提醒 + 主进程调度 + 稍后 5 分钟 + 免打扰补发（FR-REM-001/002/004） | ✅ |
| 主动搭话（FR-REM-005） | ✅ |
| 托盘 / 全局快捷键 / 开机自启（FR-SYS-001/002, FR-BUB-016/017） | ✅ |
| 本地持久化 / 会话检索 / 导出 Markdown / 配置导入导出（FR-DAT） | ✅（JSON 持久化） |

已知简化项：动画为内置 SVG 矢量（APNG/GIF/Live2D 皮肤包待后续）；数据为 JSON 文件（SQLite 待后续）；无自动更新与 i18n（P2 待后续）。

## 数据与隐私

- 数据目录：`%APPDATA%\com.mrtransition.deskpet\`（data.json / deskpet.log / exports\）
- 重置：托盘 → 重置全部数据…（或设置 → 会话与数据 → 危险区）
- 零上报、零埋点；密钥仅存系统凭据管理器

## 开发构建

工具链：Rust（`x86_64-pc-windows-gnu` 目标）+ winlibs GCC（binutils ≥ 2.41）。

> ⚠️ 不能使用 TDM-GCC 10.3（binutils 2.36）：其 PE 链接器存在长名 section 布局 bug，产物为非法 PE（运行报「不是有效的应用程序」）。

```powershell
# 1) 安装 Rust + GNU 目标：rustup target add x86_64-pc-windows-gnu
# 2) 下载 winlibs（https://github.com/brechtsanders/winlibs_mingw/releases）
# 3) 复制 .cargo/config.example.toml 为 .cargo/config.toml，
#    将其中 linker/ar 路径改为你本机的 winlibs gcc/ar 实际路径
cargo build --release --target x86_64-pc-windows-gnu --manifest-path src-tauri/Cargo.toml
# 产物：src-tauri/target/x86_64-pc-windows-gnu/release/deskpet.exe（需与 WebView2Loader.dll 同目录分发）
```

- 镜像与链接器配置模板见 `.cargo/config.example.toml`（中国大陆网络可启用 rsproxy 镜像）
- 前端为 `src/` 下三个零构建静态页（pet.html / bubble.html / settings.html + shared.css/js），由 `tauri.conf.json` 的 `frontendDist` 嵌入 exe
- Rust 主进程逻辑见 `src-tauri/src/main.rs`（窗口管理 / 托盘 / 快捷键 / 密钥 / 三协议流式对话 / 提醒调度）
- 需求文档与浏览器模拟版原型见 `docs/`

## 已知限制

1. 全局快捷键 `Ctrl+Shift+Space` 若被其他程序占用，会记录日志并降级（托盘左键仍可唤起气泡）
2. 首次双击如遇 SmartScreen 提示，选择「仍要运行」（未做代码签名）
3. 深色壁纸下浅蓝猫对比度有限，可在设置中切换橘猫/薄荷皮肤
4. 气泡宽度固定 340px（拖拽调宽待后续版本）

## 版本历史

### v0.2.3

**「新增供应商」按钮点击无反馈**：点击后界面无任何变化，用户无法感知已进入新增模式。三处修复：

1. **反馈缺失**：初始状态本就是"新增模式"表单，点击按钮后所有可见状态不变。现点击后弹出提示 toast、名称输入框聚焦并全选（直接键入即可覆盖模板默认名）。
2. **模板下拉与表单不一致**：新增模式下模板下拉残留旧值（如停留在「演示模式」），而表单内容却是 DeepSeek 模板值。现强制同步为 `deepseek`。
3. **名称残留**：从"编辑某供应商"切换到"新增"时，名称框残留旧供应商名（如「演示模式（内置，无需 Key）」），保存会以旧名称新建错误条目。现切换时清空名称并由模板重新填入默认名。

### v0.2.2

**「配置完模型，聊天崩溃」与「Ollama 调用不通」三合一修复**（均已在真实 Ollama + llama3.2:3b 上端到端验证）：

1. **崩溃根因**：构造请求头时对 API Key 执行 `parse().unwrap()`——粘贴的 Key 混入空格/换行/中文等非法字符时直接 panic，release 版 `panic="abort"` 导致整个进程静默消失。现改为返回错误提示（「API Key 含非法字符…」），同时全量移除状态锁的 `unwrap`（锁中毒防护，任一持锁线程 panic 后应用仍可运行）。
2. **Ollama 测试不通**：`provider_test` 强制要求 API Key，本地服务（Ollama / LM Studio / vLLM）无需 Key 被直接拦截。现改为：本地地址（localhost/127.0.0.1 等）无 Key 照常测试；云端无 Key 也放行，由服务端 401 给出准确反馈。实测：`✓ 连接成功 · 延迟 96ms · 16 个可用模型（本地服务，无需 Key）`。
3. **配好模型仍是演示模式**：人格绑定演示模式后无迁移路径。现改为：供应商「设为默认」保存时，自动把仍绑定演示模式的人格、会话与上次选择迁移到该供应商（toast 提示迁移数量）；演示模式对话时若检测到已配置真实供应商，气泡内会提示切换。
4. Key 输入框粘贴自动去首尾空白；新增 `cargo test` 回归（坏 Key 返回错误而非 panic 等用例）。

### v0.2.1

**设置窗口首次打开白屏**：v0.2.0 的设置窗口为懒创建——首次打开时才构建 WebView2（一组全新渲染进程冷启动），窗口先以原生白底出现、内容延迟数秒才到达。v0.2.1 改为**启动时预创建隐藏窗口**（与气泡窗口同模式），页面在后台完成加载与数据初始化，首次打开即刻显示完整界面；窗口同时默认居中显示。诊断过程中已通过 UIAutomation 验证隐藏状态下 DOM（含供应商动态卡片）全部就绪。

## 许可

[CC BY 4.0](LICENSE)：可自由共享与改编，仅需署名（附本仓库链接）。

> 本项目由 AI 协作开发。

> AI生成
