import { Buffer } from 'node:buffer'
// 极简 asar 解包器：asar = [4字节头长度][4字节头长度][4字节JSON长度][JSON头][文件数据...]
// 只依赖 node 标准库，不用装任何东西。
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'

const src = process.argv[2]
const outDir = process.argv[3]
const filterRe = process.argv[4] ? new RegExp(process.argv[4]) : null

if (!src || !outDir) {
  console.error('用法: node asar-extract.mjs <app.asar> <输出目录> [过滤正则]')
  process.exit(1)
}

const fd = fs.openSync(src, 'r')
const sizeBuf = Buffer.alloc(16)
fs.readSync(fd, sizeBuf, 0, 16, 0)

// 布局：[u32=4][u32=头总长][u32=头总长+4?][u32=JSON长度][JSON...]
// 实测 JSON 从偏移 16 开始，长度取 u32@12。
const headerTotal = sizeBuf.readUInt32LE(4)
const jsonSize = sizeBuf.readUInt32LE(12)
const headerBuf = Buffer.alloc(jsonSize)
fs.readSync(fd, headerBuf, 0, jsonSize, 16)

const header = JSON.parse(headerBuf.toString('utf8'))
// 文件数据起点 = 8 + headerTotal（实测校验）
const dataStart = 8 + headerTotal

let written = 0
let skipped = 0

function walk(node, relPath) {
  for (const [name, entry] of Object.entries(node.files || {})) {
    const current = relPath ? `${relPath}/${name}` : name
    if (entry.files) {
      walk(entry, current)
      continue
    }
    if (filterRe && !filterRe.test(current)) {
      skipped++
      continue
    }
    // 软链接条目没有 size/offset，跳过（不影响源码分析）
    const offset = Number(entry.offset)
    const size = Number(entry.size)
    if (!Number.isFinite(offset) || !Number.isFinite(size)) {
      skipped++
      continue
    }
    const target = path.join(outDir, current)
    fs.mkdirSync(path.dirname(target), { recursive: true })
    const data = Buffer.alloc(size)
    if (size > 0) {
      fs.readSync(fd, data, 0, size, dataStart + offset)
    }
    fs.writeFileSync(target, data)
    written++
    if (written % 200 === 0) console.error(`... 已解出 ${written} 个文件`)
  }
}

walk(header, '')
fs.closeSync(fd)

// eslint-disable-next-line no-console
console.log(`完成：写出 ${written} 个文件，跳过 ${skipped} 个 → ${outDir}`)
