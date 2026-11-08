#!/usr/bin/env node
/**
 * 用法: pnpm release:bump 2.0.3
 * 工作区必须干净。改版本并同步后提交、打 annotated tag vX.Y.Z，再推送当前分支和 tag。
 */
import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const ver = process.argv[2]
if (!ver || !/^\d+\.\d+\.\d+$/.test(ver)) {
  console.error('用法: pnpm release:bump <semver>  例如 pnpm release:bump 2.0.3')
  process.exit(1)
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const tag = `v${ver}`
const versionFiles = [
  'package.json',
  'src-tauri/tauri.conf.json',
  'src-tauri/Cargo.toml',
  'src-tauri/Cargo.lock',
]

function git(args, { allowStatus = null } = {}) {
  const result = spawnSync('git', args, {
    cwd: root,
    encoding: 'utf8',
  })
  if (result.error) {
    console.error(result.error.message)
    process.exit(1)
  }
  const status = result.status ?? 1
  if (status !== 0 && !(allowStatus && allowStatus.includes(status))) {
    const detail = (result.stderr || result.stdout || '').trim()
    console.error(detail || `git ${args.join(' ')} 失败`)
    process.exit(status)
  }
  return { status, stdout: (result.stdout || '').trim(), stderr: (result.stderr || '').trim() }
}

const branch = git(['rev-parse', '--abbrev-ref', 'HEAD']).stdout
if (branch === 'HEAD') {
  console.error('当前是 detached HEAD，不能发版')
  process.exit(1)
}

const dirty = git(['status', '--porcelain']).stdout
if (dirty) {
  console.error('工作区有未提交的改动，先提交或暂存后再发版')
  console.error(dirty)
  process.exit(1)
}

const pkgPath = join(root, 'package.json')
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'))
if (pkg.version === ver) {
  console.error(`package.json 已经是 ${ver}`)
  process.exit(1)
}

const localTag = git(['rev-parse', '--verify', '--quiet', `refs/tags/${tag}`], { allowStatus: [1] })
if (localTag.status === 0) {
  console.error(`本地已有 tag ${tag}`)
  process.exit(1)
}

const remoteTag = git(['ls-remote', '--exit-code', '--tags', 'origin', `refs/tags/${tag}`], {
  allowStatus: [2],
})
if (remoteTag.status === 0) {
  console.error(`远端已有 tag ${tag}`)
  process.exit(1)
}

pkg.version = ver
writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`)

const synced = spawnSync(process.execPath, [join(root, 'scripts/sync-version.mjs')], {
  cwd: root,
  stdio: 'inherit',
})
if ((synced.status ?? 1) !== 0) process.exit(synced.status ?? 1)

const notesRel = `docs/releases/v${ver}.md`
const notesGen = spawnSync(process.execPath, [join(root, 'scripts/gen-release-notes.mjs'), '--write', notesRel], {
  cwd: root,
  stdio: 'inherit',
})
if ((notesGen.status ?? 1) !== 0) process.exit(notesGen.status ?? 1)
const notesBody = readFileSync(join(root, notesRel), 'utf8').trim()
git(['add', '--', ...versionFiles, notesRel])
git(['commit', '-m', `chore: 版本 bump 至 ${ver}`])
git(['tag', '-a', tag, '-m', notesBody || tag])
git(['push', 'origin', branch])
git(['push', 'origin', tag])

console.log(`已推送 ${branch} 和 ${tag}`)
