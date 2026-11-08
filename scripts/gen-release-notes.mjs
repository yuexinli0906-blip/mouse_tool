#!/usr/bin/env node
/**
 * 根据上一 tag → HEAD 的 conventional commits 生成发版说明（主题引用 + 分类）。
 * 用法:
 *   node scripts/gen-release-notes.mjs              # 打到 stdout
 *   node scripts/gen-release-notes.mjs --write PATH # 写入文件
 *   node scripts/gen-release-notes.mjs --from v3.4.0 --to HEAD
 *
 * 若存在 docs/release-notes.override.md 且非空，优先用该文件（人工润色）。
 */
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

function git(args) {
  const r = spawnSync('git', args, { cwd: root, encoding: 'utf8' })
  if (r.error) throw r.error
  if ((r.status ?? 1) !== 0) {
    throw new Error((r.stderr || r.stdout || `git ${args.join(' ')} failed`).trim())
  }
  return (r.stdout || '').trim()
}

const args = process.argv.slice(2)
let writePath = null
let from = null
let to = 'HEAD'
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--write') writePath = args[++i]
  else if (args[i] === '--from') from = args[++i]
  else if (args[i] === '--to') to = args[++i]
}

const overridePath = join(root, 'docs/release-notes.override.md')
if (existsSync(overridePath)) {
  const override = readFileSync(overridePath, 'utf8').trim()
  if (override && !override.startsWith('<!--')) {
    if (writePath) {
      mkdirSync(dirname(join(root, writePath)), { recursive: true })
      writeFileSync(join(root, writePath), `${override}\n`, 'utf8')
    } else {
      process.stdout.write(`${override}\n`)
    }
    process.exit(0)
  }
}

if (!from) {
  try {
    from = git(['describe', '--tags', '--abbrev=0', `${to}^`])
  } catch {
    from = git(['rev-list', '--max-parents=0', 'HEAD']).split(/\r?\n/)[0]
  }
}

const range = `${from}..${to}`
const log = git(['log', range, '--pretty=format:%s'])
const subjects = log ? log.split(/\r?\n/).filter(Boolean) : []

const cats = {
  feat: { title: '✨ 新功能 · Features', items: [] },
  fix: { title: '🐛 修复 · Fixed', items: [] },
  perf: { title: '⚡ 性能 · Performance', items: [] },
  refactor: { title: '♻️ 重构 · Refactor', items: [] },
  docs: { title: '📝 文档 · Docs', items: [] },
  other: { title: '📦 其他 · Other', items: [] },
}

const skip = /^(chore(\(.+\))?!?\s*:\s*(版本\s*bump|release|bump)\b)/i

function classify(subject) {
  const m = subject.match(/^(feat|fix|perf|refactor|docs|chore|style|test|ci|build)(\(.+\))?!?\s*:\s*(.+)$/i)
  if (!m) return { type: 'other', text: subject.replace(/\s*\(#\d+\)\s*$/, '').trim() }
  const type = m[1].toLowerCase()
  const text = m[3].replace(/\s*\(#\d+\)\s*$/, '').trim()
  if (type === 'feat') return { type: 'feat', text }
  if (type === 'fix') return { type: 'fix', text }
  if (type === 'perf') return { type: 'perf', text }
  if (type === 'refactor') return { type: 'refactor', text }
  if (type === 'docs') return { type: 'docs', text }
  if (type === 'chore' || type === 'style' || type === 'test' || type === 'ci' || type === 'build') {
    return null
  }
  return { type: 'other', text: subject.replace(/\s*\(#\d+\)\s*$/, '').trim() }
}

for (const s of subjects) {
  if (skip.test(s.trim())) continue
  if (/^style(\(.+\))?!?\s*:?\s*$/i.test(s.trim()) || s.trim() === 'style') continue
  const c = classify(s)
  if (!c || !c.text) continue
  const { type, text } = c
  const bucket = cats[type] || cats.other
  if (!bucket.items.includes(text)) bucket.items.push(text)
}

const ordered = ['feat', 'fix', 'perf', 'refactor', 'docs', 'other']
const themeSource =
  cats.feat.items[0] || cats.fix.items[0] || cats.perf.items[0] || cats.other.items[0] || '日常维护与小改进'

const lines = [
  `> **本版主题**：${themeSource}`,
  `>`,
  `> **Theme**: ${themeSource}`,
  ``,
]

for (const key of ordered) {
  const cat = cats[key]
  if (!cat.items.length) continue
  lines.push(`### ${cat.title}`, ``)
  for (const item of cat.items) {
    lines.push(`- **${item}**`)
  }
  lines.push(``)
}

lines.push(`## 📦 安装 / Install`, ``)
lines.push(`安装包在下方 **Assets**；应用内更新使用 \`latest.json\`。`, ``)
lines.push(`Installers are in **Assets** below. In-app updater uses \`latest.json\`.`, ``)

const repo = (() => {
  try {
    const url = git(['remote', 'get-url', 'origin'])
    const m = url.match(/github\.com[:/](.+?)(?:\.git)?$/)
    return m ? m[1] : 'wjyugutou/mouse_tool'
  } catch {
    return 'wjyugutou/mouse_tool'
  }
})()

const toTag = (() => {
  try {
    if (to === 'HEAD') return git(['describe', '--tags', '--exact-match', 'HEAD'])
  } catch {
    /* not on a tag */
  }
  return to === 'HEAD' ? 'HEAD' : to
})()

if (from.startsWith('v') || from.length < 40) {
  const right = toTag.startsWith('v') ? toTag : `v${toTag}`
  const left = from.startsWith('v') ? from : from
  if (right.startsWith('v')) {
    lines.push(`**Full Changelog**: https://github.com/${repo}/compare/${left}...${right}`, ``)
  }
}

const body = `${lines.join('\n').trim()}\n`

if (writePath) {
  const abs = join(root, writePath)
  mkdirSync(dirname(abs), { recursive: true })
  writeFileSync(abs, body, 'utf8')
  console.error(`wrote ${writePath}`)
} else {
  process.stdout.write(body)
}
