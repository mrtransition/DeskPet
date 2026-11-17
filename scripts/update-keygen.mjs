#!/usr/bin/env node
/**
 * DeskPet 更新签名密钥生成（NFR-SEC-004）
 *
 * 用法：
 *   node scripts/update-keygen.mjs [--out .temp/deskpet-update-signing-key.pem] [--force]
 *
 * 产物：
 *   - 私钥 PEM（默认写到 .temp/，已被 .gitignore 忽略；请离线备份，不要提交）
 *   - 公钥 base64（32 字节 Ed25519 raw key，贴到 main.rs 的 UPDATE_ED25519_PUBKEYS）
 *   - 公钥指纹（SHA-256，便于发布说明中核对）
 */

import { createHash, generateKeyPairSync } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const args = process.argv.slice(2);
const force = args.includes('--force');
const outIndex = args.indexOf('--out');
const out = resolve(outIndex >= 0 && args[outIndex + 1] ? args[outIndex + 1] : '.temp/deskpet-update-signing-key.pem');

if (existsSync(out) && !force) {
  console.error(`私钥已存在：${out}\n如需重新生成，请加 --force（旧密钥对应的公钥将失效，已发布版本无法验证新签名）。`);
  process.exit(1);
}

const { publicKey, privateKey } = generateKeyPairSync('ed25519');
const privatePem = privateKey.export({ type: 'pkcs8', format: 'pem' });
const spki = publicKey.export({ type: 'spki', format: 'der' });
const raw = spki.subarray(spki.length - 32);
const rawB64 = raw.toString('base64');
const fingerprint = createHash('sha256').update(spki).digest('hex');

mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, privatePem, { mode: 0o600 });
writeFileSync(`${out}.pub`, `${rawB64}\n${fingerprint}\n`, { mode: 0o644 });

console.log('已生成 Ed25519 更新签名密钥：');
console.log(`  私钥：${out}（离线备份，禁止提交）`);
console.log(`  公钥（base64）：${rawB64}`);
console.log(`  公钥指纹：${fingerprint}`);
console.log('');
console.log('把公钥 base64 加入 src-tauri/src/main.rs 的 UPDATE_ED25519_PUBKEYS 后重新编译；');
console.log('发布时运行：node scripts/sign-update.mjs dist/DeskPet-vX.Y.Z-win-x64.zip');
