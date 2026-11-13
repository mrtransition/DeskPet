#!/usr/bin/env node
/**
 * DeskPet 治理基线检查
 *
 * 目标：把 P-01 ~ P-05 的“文档存在”变成可执行的 CI 门禁，而不是一次性交付。
 * 检查项：
 *   1. 治理基线必需文件存在；
 *   2. docs/releases/*.md 包含发布说明必需章节，且“收敛与移除”非空；
 *   3. 安全评审记录包含六个维度与结论；
 *   4. 安全评审登记表与 INT-xxx 文件一一对应；
 *   5. 治理文档中的相对 Markdown 链接可解析。
 *
 * 用法：node scripts/check-governance.mjs
 */

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const errors = [];

function fail(message) {
  errors.push(message);
}

function rel(path) {
  return path.slice(root.length + 1).replace(/\\/g, '/');
}

function read(relPath) {
  return readFileSync(join(root, relPath), 'utf8');
}

// ---------- 1. 必需文件 ----------
const requiredFiles = [
  'docs/治理/README.md',
  'docs/治理/需求准入检查清单.md',
  'docs/治理/默认开启能力清单.md',
  'docs/治理/工具调用能力边界.md',
  'docs/治理/隐私与安全白皮书.md',
  'docs/治理/安全评审流程.md',
  'docs/治理/发布说明规范.md',
  'docs/治理/发布说明模板.md',
  'docs/治理/安全评审/README.md',
  'docs/治理/安全评审/_模板.md',
  'docs/releases/README.md',
  'docs/releases/v2.3.0.md',
  'CONTRIBUTING.md',
  '.github/PULL_REQUEST_TEMPLATE.md',
  '.github/ISSUE_TEMPLATE/feature_request.yml',
  '.github/workflows/governance.yml',
];

for (const file of requiredFiles) {
  if (!existsSync(join(root, file))) fail(`缺少治理基线文件：${file}`);
}

// ---------- 2. 发布说明 ----------
const releaseSections = [
  '## 摘要',
  '## 新增',
  '## 修复',
  '## 收敛与移除',
  '## 安全与隐私变更',
  '## 验证',
  '## 升级说明',
];

const releasesDir = join(root, 'docs/releases');
if (existsSync(releasesDir)) {
  for (const name of readdirSync(releasesDir)) {
    if (!/^v\d+\.\d+\.\d+\.md$/.test(name)) continue;
    const file = `docs/releases/${name}`;
    const text = read(file);
    for (const section of releaseSections) {
      if (!text.includes(section)) fail(`${file} 缺少发布说明章节：${section}`);
    }
    const body = text.split('## 收敛与移除')[1]?.split(/\n## /)[0] ?? '';
    const hasItem = /^\s*-\s+\S/m.test(body);
    if (!body.trim() || (!hasItem && !body.includes('本版本无'))) {
      fail(`${file} 的“收敛与移除”章节为空；无内容时必须写“本版本无”`);
    }
  }
}

// ---------- 3 & 4. 安全评审记录 ----------
const reviewDir = join(root, 'docs/治理/安全评审');
const reviewChecks = [
  ['## 2. 数据流向', '数据流向'],
  ['## 3. 权限范围', '权限范围'],
  ['## 4. 授权与撤销', '授权与撤销'],
  ['## 5. 日志与审计', '日志与审计'],
  ['## 6. 失败降级', '失败降级'],
  ['## 7. 风险评级', '风险评级'],
  ['## 9. 结论', '结论'],
];

const recordIds = new Set();
if (existsSync(reviewDir)) {
  for (const name of readdirSync(reviewDir)) {
    if (!/^INT-\d{3}-.+\.md$/.test(name)) continue;
    const id = name.slice(0, 7);
    recordIds.add(id);
    const file = `docs/治理/安全评审/${name}`;
    const text = read(file);
    for (const [heading, label] of reviewChecks) {
      if (!text.includes(heading)) fail(`${file} 缺少评审维度：${label}`);
    }
    if (!/[✅🟡❌⏸]/.test(text)) fail(`${file} 缺少明确结论标记（✅ / 🟡 / ❌ / ⏸）`);
    if (!text.includes(`| ${id} |`)) fail(`${file} 基本信息表缺少编号：${id}`);
  }
}

const registryPath = join(reviewDir, 'README.md');
if (existsSync(registryPath)) {
  const registry = readFileSync(registryPath, 'utf8');
  const registered = new Set(
    [...registry.matchAll(/^\|\s*(INT-\d{3})\s*\|/gm)].map((m) => m[1]),
  );
  for (const id of recordIds) {
    if (!registered.has(id)) fail(`安全评审登记表缺少 ${id}（记录文件已存在）`);
  }
  for (const id of registered) {
    if (!recordIds.has(id)) fail(`安全评审登记表引用了不存在的 ${id} 记录`);
  }
}

// ---------- 5. 相对链接 ----------
function walkMarkdown(dir) {
  const out = [];
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...walkMarkdown(full));
    else if (extname(name).toLowerCase() === '.md') out.push(full);
  }
  return out;
}

const linkFiles = [
  join(root, 'README.md'),
  join(root, 'CONTRIBUTING.md'),
  join(root, '.github/PULL_REQUEST_TEMPLATE.md'),
  ...walkMarkdown(join(root, 'docs/治理')),
  ...walkMarkdown(join(root, 'docs/releases')),
].filter((p) => existsSync(p));

const linkPattern = /\[[^\]]*\]\(([^)]+)\)/g;
const seen = new Set();

for (const file of linkFiles) {
  if (seen.has(file)) continue;
  seen.add(file);
  let text = readFileSync(file, 'utf8');
  // 忽略代码块中的示例链接
  text = text.replace(/```[\s\S]*?```/g, '');
  for (const match of text.matchAll(linkPattern)) {
    let target = match[1].trim();
    if (!target || target.startsWith('#')) continue;
    if (/^(https?:|mailto:|tel:)/i.test(target)) continue;
    target = target.replace(/^<|>$/g, '');
    target = target.split('#')[0].split('?')[0];
    if (!target) continue;
    try {
      target = decodeURIComponent(target);
    } catch {
      // 保留原始值，交给存在性检查
    }
    const absolute = target.startsWith('/')
      ? join(root, target)
      : resolve(dirname(file), target);
    if (!existsSync(absolute)) {
      fail(`${rel(file)} 的相对链接无法解析：${match[1]}`);
    }
  }
}

// ---------- 结果 ----------
if (errors.length > 0) {
  console.error(`\n✗ 治理基线检查失败（${errors.length} 项）：\n`);
  for (const error of errors) console.error(`  - ${error}`);
  console.error('\n修复后重新运行：node scripts/check-governance.mjs\n');
  process.exit(1);
}

console.log('✓ 治理基线检查通过');
console.log(`  - 必需文件：${requiredFiles.length} 项`);
console.log(`  - 发布说明：${readdirSync(releasesDir).filter((n) => /^v\d+\.\d+\.\d+\.md$/.test(n)).length} 份`);
console.log(`  - 安全评审记录：${recordIds.size} 份，登记表一致`);
console.log(`  - Markdown 链接：${seen.size} 个文件已检查`);
