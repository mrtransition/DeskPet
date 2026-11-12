# 方案：将极简图片背景去除工具合并到 DeskPet

> 本文档供 AI 审阅和修改。目标是评估并完善把“纯前端图片背景去除工具”集成进 DeskPet 的技术方案。

---

## 1. 背景与目标

### 1.1 项目现状

- 现有皮肤导入流程支持 GIF / APNG / 动态 WebP，并内置“自动裁剪四周留白与棋盘格底色”逻辑。
- 对于背景为纯色（白墙、绿幕、蓝幕）或简单场景的普通图片，用户仍需借助外部工具手动抠图，流程割裂。

### 1.2 目标

在 DeskPet 的**皮肤/素材导入流程**中，增加一个**可选的“智能去除背景”步骤**，让用户直接导入 JPG/PNG 等普通图片，由工具自动去除背景并输出透明通道素材，随后无缝进入现有的“自动裁剪 → 统一画布对齐 → 生成 `skin.json` 皮肤帧”流程。

### 1.3 非目标

- 不引入任何深度学习模型或外部依赖。
- 不做通用复杂场景抠图（如人物发丝、毛发、半透明物体），定位为**纯色/简单背景**去除。
- 不改变 DeskPet 现有的皮肤规范与导入主流程，仅插入可选步骤。

---

## 2. 总体方案

### 2.1 功能定位

背景去除不是独立页面，而是**皮肤导入向导中的一个可选步骤**：

> 导入图片 → （可选）智能去背景 → 自动裁剪/对齐 → 生成皮肤

### 2.2 两种集成路径

| 路径                                 | 做法                                                                                                  | 优点                                                         | 缺点                           |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------------------------------ |
| **A. 前端 WebView 集成（推荐先做）** | 将现有 JS 算法原样运行在 DeskPet 前端 WebView 中，Canvas 处理，输出透明 PNG DataURL，交给现有导入逻辑 | 零后端改动，合并成本最低；Chromium 内核支持 Canvas/ImageData | 超大图（>2K）速度不如 Rust     |
| **B. Rust 后端集成（进阶）**         | 将核心算法用 Rust 重写，新增 Tauri `remove_background` 命令，前端传 RGBA 或文件路径，后端返回透明 PNG | 性能好、内存可控，适合大图与批量处理                         | 需要改 Rust 后端，合并成本较高 |

**建议**：先以路径 A 合并，验证用户需求与效果；若性能或批量处理成为瓶颈，再迁移到路径 B。算法为纯像素操作，A → B 移植成本低。

---

## 3. 核心算法（与 UI 解耦，可移植）

### 3.1 输入输出

```text
输入: RGBA 像素数组 data, width, height
参数: tolerance（颜色容差）, feather（羽化半径）, edgeSample（边缘采样带）
输出: 原地修改的 data（背景像素 Alpha 置 0 或降低）+ 背景色估计值
```

### 3.2 流程

1. **估计背景色**：从图像四条边界取宽度为 `edgeSample` 的边缘带，统计量化颜色直方图，取出现次数最多的颜色作为背景色 `bg`。
2. **构建背景掩码**：从四边像素开始洪泛填充，颜色与 `bg` 的欧氏距离 ≤ `tolerance × 1.25` 的连通区域标记为背景。
3. **应用 Alpha**：掩码内像素按距离映射 Alpha：距离 ≤ `tolerance` 置 0；`tolerance ~ 1.25×tolerance` 线性过渡；只降低 Alpha，不提高。
4. **边缘羽化**：对 Alpha 通道做可分离盒式模糊（水平 + 垂直），半径 `feather`。

### 3.3 核心伪代码

```pseudo
function removeBackground(imageData, options):
    bg = estimateBackgroundColor(data, width, height, options.edgeSample)
    mask = buildBackgroundMask(data, width, height, bg, options.tolerance)
    removed = applyAlpha(data, mask, width, height, bg, options.tolerance)
    featherEdge(data, width, height, round(options.feather))
    return { backgroundColor: bg, removedRatio: removed / (width * height) }

function estimateBackgroundColor(data, width, height, band):
    bins = map  // 量化颜色 -> [count, sumR, sumG, sumB]
    band = max(1, min(band, min(width, height) / 2))
    遍历四条边缘带（宽 band），跳过 alpha < 8 的像素，累加量化颜色
    return 出现次数最多的 bin 的平均 RGB（无则白色）

function buildBackgroundMask(data, width, height, bg, tolerance):
    mask = Uint8Array(width * height)
    limit = tolerance * 1.25
    从四边像素入栈，洪泛填充：
        若 alpha == 0 或 RGB 与 bg 距离平方 <= limit²，标记 mask=1 并继续扩散
    return mask

function applyAlpha(data, mask, width, height, bg, tolerance):
    t0 = tolerance; t1 = tolerance * 1.25; span = max(1, t1 - t0)
    for 每个 mask=1 的像素:
        d = RGB 与 bg 的欧氏距离
        a = d <= t0 ? 0 : (d - t0) / span
        if a < 1: data.alpha = min(data.alpha, round(a * 255))
    return 完全透明像素数

function featherEdge(data, width, height, radius):
    if radius <= 0: return
    提取 alpha 通道 → 水平盒式模糊 → 垂直盒式模糊 → 写回 alpha
```

### 3.4 Rust 移植要点（路径 B）

- 输入用 `&mut [u8]`（RGBA）或 `Vec<u8>`，宽高 `u32`。
- 洪泛填充用 `Vec<u8>` 掩码 + `Vec<i32>` 栈或 `VecDeque`。
- 直方图用 `HashMap<u32, [u64; 4]>`，key 为量化 RGB。
- 羽化用两个 `Vec<u8>` 临时缓冲，或原地滑动窗口。
- Tauri 命令示例签名：

  ```rust
  #[tauri::command]
  fn remove_background(rgba: Vec<u8>, width: u32, height: u32, tolerance: u8, feather: u8, edge_sample: u8) -> Result<Vec<u8>, String>
  ```

  前端通过 `invoke` 调用，返回处理后的 RGBA 再编码为 PNG。

---

## 4. 前端 UI / UX 设计

### 4.1 集成位置

- 在 DeskPet 的**皮肤导入页面/弹窗**中，增加“智能去除背景”按钮或可选步骤。
- 默认**跳过**，不改变现有用户流程。

### 4.2 交互流程

1. 用户选择/拖入图片（复用现有拖拽上传能力）。
2. 显示原图与处理结果双栏预览（棋盘格透明底）。
3. 提供三个滑块：**颜色容差**、**边缘羽化**、**边缘采样带**，实时重新处理。
4. 按钮：“应用去背景结果” / “跳过” / “下载 PNG”。
5. 应用后，将透明 PNG 交给现有导入流程继续（自动裁剪、对齐、生成皮肤）。

### 4.3 现有代码可复用部分

- 全页拖拽上传（`document` 级 `dragenter/dragover/dragleave/drop` + 遮罩）。
- 双 Canvas 预览（原图 + 结果，棋盘格背景）。
- 参数联动实时处理。
- 粘贴图片（`Ctrl+V`）可选保留。

---

## 5. 集成改动清单

### 5.1 路径 A（前端 WebView）

- 新增 `src/js/background-remover.js`（核心算法 + UI 绑定，可模块化）。
- 在皮肤导入页面引入该脚本，增加去背景区域。
- 处理结果通过 `canvas.toDataURL('image/png')` 或 `toBlob` 交给现有导入函数。
- **无 Rust 改动**。

### 5.2 路径 B（Rust 后端）

- 新增 `src-tauri/src/background_remover.rs`，实现核心算法。
- 在 `main.rs` 注册 `remove_background` 命令。
- 前端改为 `invoke('remove_background', { ... })`，接收 RGBA 后绘制到 Canvas 再导出。
- 注意 Tauri IPC 传输大数组的性能，可考虑传文件路径而非原始像素（若前端能拿到临时文件路径）。

---

## 6. 测试要点

- **效果**：纯白 / 绿幕 / 蓝幕背景的 JPG/PNG，去背景后边缘是否干净；容差与羽化滑块是否有效。
- **性能**：1024×1024、2048×2048 图片的处理耗时（路径 A vs 路径 B）。
- **兼容性**：与现有 GIF/APNG 导入流程共存；去背景步骤默认跳过时不影响原流程。
- **边界**：全透明图片、单色图片、超大图、非图片文件拖入的提示。
- **内存**：路径 A 下大图 Canvas 内存占用；路径 B 下 IPC 数据量。

---

## 7. 风险与备选

| 风险               | 缓解                                                              |
| ------------------ | ----------------------------------------------------------------- |
| 复杂背景效果差     | 明确定位为纯色/简单背景；UI 提示“建议纯色背景图片”                |
| 大图前端卡顿       | 限制最大边 1400px 再处理；或路径 B Rust 处理                      |
| 与现有自动裁剪冲突 | 去背景输出透明 PNG 后，自动裁剪逻辑对透明边界应能正确处理，需验证 |
| Tauri IPC 大数据量 | 路径 B 优先传文件路径；或分块传输                                 |

---

## 8. 待 AI 修改/补充的问题

1. 路径 A 与路径 B 的取舍是否合理？是否应先做路径 A？
2. 前端算法是否需要 Worker 避免阻塞 UI？
3. 与 DeskPet 现有“自动裁剪棋盘格底色”逻辑是否存在功能重叠，能否合并？
4. `skin.json` 生成流程对透明通道是否有特殊要求？
5. Rust 实现中，`tolerance` / `feather` / `edgeSample` 的参数类型与默认值是否合适？
6. 是否需要支持批量导入多张图片依次去背景？
7. 是否有更合适的集成位置（如独立设置页 vs 导入弹窗）？

---

## 9. 落地实现记录（本次迭代）

已按**路径 A** 完成首版合并，不引入 Rust 改动、深度学习模型或新依赖。

### 9.1 文件与集成位置

| 改动 | 说明 |
| --- | --- |
| `src/background-remover.js`（新增） | 纯算法模块，暴露 `window.BgRemover.removeBackground(imageData, options)`；包含背景色估计、洪泛掩码、Alpha 映射与可分离盒式羽化。与 UI 完全解耦，后续可平移到 Rust。 |
| `src/settings.html` | 在「宠物与交互 → 皮肤」下方新增「智能去除背景」可选区块：双栏预览（原图 / 棋盘格透明结果）、三个参数滑块、应用 / 下载 / 恢复默认 / 收起预览。 |
| `src/i18n.js` | 新增 `bg.*` 中英文词条，静态文案与状态提示均接入 `t()`。 |
| 复用 | 文件读取、缩放、自动裁剪、八态单图切割与皮肤包落库全部复用现有函数；去背景结果通过 `petImageSplitEight(dataUrl)` 直接进入 4×2 切割流程。 |
| 下载 | 复用主进程 `skin_image_export`（写入 `exports/` 并 `reveal_file`），不新增 Rust 命令。 |

### 9.2 实现决策

1. **先做路径 A**：算法为纯像素操作，前端 1600×1600 实测约 150 ms（Node/V8 基准），满足当前单图导入场景；批量处理或 >2K 大图再迁移路径 B。
2. **不使用 Worker**：`tauri.conf.json` 的 CSP 为 `worker-src 'none'`，首版用「输入防抖 + 下一帧执行」避免拖动滑块时连续阻塞；若后续放开 Worker，可在不改算法接口的前提下迁移。
3. **与自动裁剪不重叠**：自动裁剪解决的是「留白 / 棋盘格底色」的包围盒问题，去背景解决的是「实心纯色 / 绿幕背景」的 Alpha 通道问题。去背景输出透明 PNG 后，自动裁剪走「透明图取 Alpha 包围盒」分支，两者顺序衔接而非重复实现。
4. **默认值**：`tolerance = 32`、`feather = 2`、`edgeSample = 12`；滑块范围分别为 0–120 / 0–10 / 2–64。处理长边上限 1600 px，输入文件上限沿用自动裁剪的 48 MB。
5. **批量导入**：本次不做。批量八态图片维护已有独立入口，去背景按单张交互更可控。
6. **集成位置**：放在设置页皮肤区块而不是独立页面，符合「导入向导中的一个可选步骤」定位，也避免新增窗口与 IPC 权限。

### 9.3 后续可选增强

- 对大图或批量文件增加 Rust `remove_background` 命令（路径 B），前端仅传文件路径；
- 支持对「批量导入八态图片」逐张去背景；
- 增加背景色手动拾取（点击图片取色）与「反选保留区域」，覆盖背景与主体颜色接近的场景。

