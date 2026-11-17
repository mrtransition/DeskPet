#!/usr/bin/env node
/**
 * DeskPet 更新包签名与哈希（NFR-SEC-004）
 *
 * 用法：
 *   node scripts/sign-update.mjs <安装包> [--key .temp/deskpet-update-signing-key.pem] [--no-sidecar]
 *
 * 输出可直接粘贴到 GitHub Release 正文末尾的校验行：
 *   SHA256: <64 位小写 hex>
 *   Ed25519-Signature: <base64>
 *
 * 签名对象：SHA256 的 64 位小写 hex 字符串（UTF-8 字节），不是文件原文。
 * 同时写出 <安装包>.sha256 与 <安装包>.sig（可用 --no-sidecar 关闭）。
 */

import { createHash, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const args = process.argv.slice(2);
const artifact = args.find((a) => !a.startsWith('--') && args[args.indexOf(a) - 1] !== '--key');
const keyIndex = args.indexOf('--key');
const keyPath = resolve(keyIndex >= 0 && args[keyIndex + 1] ? args[keyIndex + 1] : '.temp/deskpet-update-signing-key.pem');
const noSidecar = args.includes('--no-sidecar');

if (!artifact) {
  console.error('用法：node scripts/sign-update.mjs <安装包> [--key <私钥 PEM>] [--no-sidecar]');
  process.exit(1);
}

const file = resolve(artifact);
const bytes = readFileSync(file);
const sha256 = createHash('sha256').update(bytes).digest('hex');
const privateKey = createPrivateKey(readFileSync(keyPath));
const signature = sign(null, Buffer.from(sha256, 'utf8'), privateKey).toString('base64');

// 自检：用私钥导出的公钥立即验签，避免写出无法验证的签名
const publicKey = createPublicKey(privateKey);
if (!verify(null, Buffer.from(sha256, 'utf8'), publicKey, Buffer.from(signature, 'base64'))) {
  console.error('签名自检失败，未写出任何文件。');
  process.exit(1);
}

const rawPub = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');

if (!noSidecar) {
  writeFileSync(`${file}.sha256`, `${sha256}  ${file.split(/[\\/]/).pop()}\n`, 'utf8');
  writeFileSync(`${file}.sig`, `${signature}\n`, 'utf8');
}

console.log(`文件：${file}`);
console.log(`大小：${bytes.length} 字节`);
console.log(`公钥：${rawPub}`);
console.log('');
console.log('把下面两行原样追加到 GitHub Release 正文末尾（更新检查会解析）：');
console.log('');
console.log(`SHA256: ${sha256}`);
console.log(`Ed25519-Signature: ${signature}`);
if (!noSidecar) {
  console.log('');
  console.log(`已写出：${file}.sha256`);
  console.log(`已写出：${file}.sig`);
}
