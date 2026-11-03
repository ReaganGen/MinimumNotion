#!/usr/bin/env node
// Generates a synthetic Forma workspace for performance testing
// (IMPLEMENTATION_PLAN §6.1 task 5). Deterministic: the same seed gives the
// same files. Never point it at real notes; it refuses a folder that isn't empty.
//
//   node scripts/generate-workspace.mjs <folder> [pages=5000] [seed=1]
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [folder, pagesArg = "5000", seedArg = "1"] = process.argv.slice(2);
if (!folder) {
  console.error("usage: node scripts/generate-workspace.mjs <folder> [pages] [seed]");
  process.exit(1);
}
if (existsSync(folder) && readdirSync(folder).length > 0) {
  console.error(`${folder} isn't empty; refusing to write into it.`);
  process.exit(1);
}
const total = Number(pagesArg);
let state = Number(seedArg) >>> 0 || 1;

/** xorshift32: small, deterministic and good enough for test data. */
function random() {
  state ^= state << 13;
  state ^= state >>> 17;
  state ^= state << 5;
  return (state >>> 0) / 4294967296;
}
const pick = (list) => list[Math.floor(random() * list.length)];
const uuid = () => {
  const hex = Array.from({ length: 32 }, () => Math.floor(random() * 16).toString(16));
  hex[12] = "4";
  hex[16] = ((Number.parseInt(hex[16], 16) & 3) | 8).toString(16);
  const h = hex.join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
};

const WORDS = (
  "attention protein design model graph embedding sequence structure folding " +
  "library garden reading notes project meeting draft review summary method " +
  "result figure table dataset training inference memory search index link " +
  "language vision kernel matrix vector cluster sample signal pattern theory"
).split(" ");
const CHINESE = [
  "注意力机制在蛋白质结构预测中的应用",
  "知识管理工具应该简单可靠",
  "今天整理了阅读笔记和项目计划",
  "模型训练需要更多的数据和时间",
  "本地优先的软件让数据属于用户",
  "检索增强生成依赖高质量的索引",
];

function sentence() {
  if (random() < 0.25) return `${pick(CHINESE)}。`;
  const length = 6 + Math.floor(random() * 12);
  const words = Array.from({ length }, () => pick(WORDS));
  words[0] = words[0][0].toUpperCase() + words[0].slice(1);
  return `${words.join(" ")}.`;
}

function title(i) {
  const base = random() < 0.2 ? pick(CHINESE).slice(0, 6) : `${pick(WORDS)} ${pick(WORDS)}`;
  return `${base[0].toUpperCase()}${base.slice(1)} ${i}`;
}

const pages = [];
function body(i) {
  const paragraphs = [];
  const count = 2 + Math.floor(random() * 5);
  for (let p = 0; p < count; p++) {
    const parts = Array.from({ length: 2 + Math.floor(random() * 4) }, sentence);
    if (pages.length > 0 && random() < 0.4) {
      const target = pick(pages);
      parts.push(`See [${target.title}](forma://page/${target.id}).`);
    }
    if (pages.length > 0 && random() < 0.15) parts.push(`Also [[${pick(pages).title}]].`);
    paragraphs.push(parts.join(" "));
  }
  if (i % 10 === 0) paragraphs.push(`- [ ] ${sentence()}\n- [x] ${sentence()}`);
  return `${paragraphs.join("\n\n")}\n`;
}

function frontmatter(id, pageTitle, extra = "") {
  const tags = random() < 0.3 ? `tags:\n  - ${pick(WORDS)}\n` : "";
  return `---\nid: ${id}\ntitle: "${pageTitle}"\ncreated_at: 2026-11-01T10:00:00Z\nupdated_at: 2026-11-01T10:00:00Z\n${tags}${extra}---\n`;
}

const safe = (s) => s.replace(/[/\\:*?"<>|]/g, "-");
mkdirSync(folder, { recursive: true });

// A collection with 10% of the pages as entries.
const collectionEntries = Math.floor(total / 10);
const collectionDir = join(folder, "Reading list");
mkdirSync(collectionDir);
writeFileSync(
  join(collectionDir, "index.md"),
  `${frontmatter(
    uuid(),
    "Reading list",
    `collection:
  version: 1
  properties:
    - id: status00
      name: Status
      type: select
      options:
        - id: tord
          name: To read
          color: gray
        - id: rdng
          name: Reading
          color: blue
        - id: done
          name: Done
          color: green
    - id: year0000
      name: Year
      type: number
    - id: read0000
      name: Read
      type: checkbox
`,
  )}Papers and books.\n`,
);
for (let i = 0; i < collectionEntries; i++) {
  const id = uuid();
  const pageTitle = title(i);
  const values = `Status: ${pick(["To read", "Reading", "Done"])}\nYear: ${1990 + Math.floor(random() * 36)}\nRead: ${random() < 0.5}\n`;
  writeFileSync(
    join(collectionDir, `${safe(pageTitle)}.md`),
    `${frontmatter(id, pageTitle, values)}${body(i)}`,
  );
  pages.push({ id, title: pageTitle });
}

// The rest: top-level sections, each with pages, some with child pages.
const sections = 50;
let made = collectionEntries;
for (let s = 0; s < sections && made < total; s++) {
  const sectionTitle = `Section ${s + 1}`;
  const dir = join(folder, sectionTitle);
  mkdirSync(dir);
  writeFileSync(join(dir, "index.md"), `${frontmatter(uuid(), sectionTitle)}${body(s)}`);
  made++;
  const perSection = Math.ceil((total - collectionEntries) / sections);
  for (let i = 0; i < perSection && made < total; i++, made++) {
    const id = uuid();
    const pageTitle = title(made);
    writeFileSync(join(dir, `${safe(pageTitle)}.md`), `${frontmatter(id, pageTitle)}${body(made)}`);
    pages.push({ id, title: pageTitle });
  }
}
console.log(`Wrote ${made} pages to ${folder}`);
