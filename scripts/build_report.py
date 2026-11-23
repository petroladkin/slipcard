#!/usr/bin/env python3
"""Builds report.html for a task's evidence report.

The report directory holds notes/*.md, logs/* and shots/*; the result is one
self-contained file (no external requests, images inlined as data URIs, logs
inlined in <details>), in the order of an evidence report:

  notes/summary.md        1. summary per acceptance criterion
  notes/decisions.md      2. decisions for the maintainer
  notes/scenarios.md      3. scenario results (a Markdown table), plus logs/*
  shots/*                 screenshots and GIFs, captioned by shots/captions.txt
  notes/not-verified.md   4. not verified
  notes/environment.md    5. environment

Missing files are skipped. shots/captions.txt has one "<file name> <caption>"
per line; a shot without a caption is captioned with its file name.

Usage: scripts/build_report.py <report-dir> <title> [<meta line>]
"""
import base64
import html
import pathlib
import re
import sys

if len(sys.argv) < 3:
    sys.exit("usage: build_report.py <report-dir> <title> [<meta line>]")
root = pathlib.Path(sys.argv[1])
title = sys.argv[2]
meta = sys.argv[3] if len(sys.argv) > 3 else ""
notes = root / "notes"
logs = root / "logs"
shots = root / "shots"


def inline(text):
    text = html.escape(text, quote=False)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
    return text


def md(src):
    """A small Markdown subset: headings, paragraphs, lists, tables, code blocks."""
    out, para, items, table, code = [], [], [], [], None
    ordered = False

    def flush():
        nonlocal para, items, table
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>")
            para = []
        if items:
            tag = "ol" if ordered else "ul"
            out.append(f"<{tag}>" + "".join(f"<li>{inline(i)}</li>" for i in items) + f"</{tag}>")
            items = []
        if table:
            rows = [r for r in table if not re.match(r"^\|[-:| ]+\|$", r)]
            cells = [[c.strip() for c in r.strip().strip("|").split("|")] for r in rows]
            head = "".join(f"<th>{inline(c)}</th>" for c in cells[0])
            body = "".join(
                "<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>" for r in cells[1:])
            out.append(f'<div class="scroll"><table><thead><tr>{head}</tr></thead>'
                       f"<tbody>{body}</tbody></table></div>")
            table = []

    for line in src.splitlines():
        if code is not None:
            if line.startswith("```"):
                out.append("<pre>" + html.escape("\n".join(code)) + "</pre>")
                code = None
            else:
                code.append(line)
            continue
        if line.startswith("```"):
            flush()
            code = []
            continue
        if line.startswith("|"):
            if para or items:
                flush()
            table.append(line)
            continue
        if table:
            flush()
        m = re.match(r"^(#{1,3}) (.*)", line)
        if m:
            flush()
            level = len(m.group(1)) + 1
            out.append(f"<h{level}>{inline(m.group(2))}</h{level}>")
        elif re.match(r"^(\d+\.|-) ", line):
            if para:
                flush()
            ordered = bool(re.match(r"^\d+\.", line))
            items.append(re.sub(r"^(\d+\.|-) ", "", line))
        elif line.startswith("  ") and items:
            items[-1] += " " + line.strip()
        elif not line.strip():
            flush()
        elif items:
            items[-1] += " " + line.strip()
        else:
            para.append(line.strip())
    flush()
    return "\n".join(out)


def note(name):
    path = notes / name
    if not path.exists():
        return ""
    body = md(path.read_text())
    return re.sub(r"^<h2>.*?</h2>", "", body, count=1)  # the file's own title


def section(heading, body):
    return f"<section><h2>{html.escape(heading)}</h2>{body}</section>" if body else ""


MIME = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg",
        ".webp": "image/webp", ".gif": "image/gif"}


def shots_html():
    if not shots.is_dir():
        return ""
    captions = {}
    captions_file = shots / "captions.txt"
    if captions_file.exists():
        for line in captions_file.read_text().splitlines():
            if line.strip():
                name, _, caption = line.partition(" ")
                captions[name] = caption.strip()
    figures = []
    for p in sorted(shots.iterdir()):
        if p.suffix.lower() not in MIME:
            continue
        uri = f"data:{MIME[p.suffix.lower()]};base64," + base64.b64encode(p.read_bytes()).decode()
        caption = captions.get(p.name, p.name)
        figures.append(f'<figure><img loading="lazy" src="{uri}" alt="{html.escape(caption)}">'
                       f"<figcaption>{html.escape(caption)}</figcaption></figure>")
    return f'<div class="shots">{"".join(figures)}</div>' if figures else ""


log_blocks = ""
if logs.is_dir():
    log_blocks = "".join(
        f"<details><summary><code>logs/{p.name}</code></summary>"
        f"<pre>{html.escape(p.read_text(errors='replace'))}</pre></details>"
        for p in sorted(logs.iterdir()) if p.is_file())

scenarios = note("scenarios.md") + shots_html() + log_blocks

page = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<style>
:root{{--bg:#F4F1EA;--panel:#EAE5DA;--ink:#1F3A8A;--text:#1D1B17;--muted:#6B655A;--rule:#D6CFC1}}
@media (prefers-color-scheme: dark){{:root{{--bg:#14130F;--panel:#1D1B16;--ink:#9DB1FF;--text:#E6E1D6;--muted:#9A9284;--rule:#2E2B24;color-scheme:dark}}}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--bg);color:var(--text);font:15px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif;padding:24px 16px 64px}}
main{{max-width:1040px;margin:0 auto}}
h1{{font-size:26px;margin:0 0 4px;color:var(--ink)}}
h2{{font-size:13px;letter-spacing:.12em;text-transform:uppercase;color:var(--muted);border-bottom:1px solid var(--rule);padding-bottom:6px;margin:36px 0 12px}}
h3{{font-size:16px;margin:20px 0 8px}}
.meta{{color:var(--muted);font:12px/1.5 ui-monospace,Menlo,monospace}}
code,pre{{font:12.5px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}}
.scroll{{overflow-x:auto}}
table{{border-collapse:collapse;width:100%;font-size:14px}}
th,td{{text-align:left;vertical-align:top;padding:7px 10px 7px 0;border-bottom:1px solid var(--rule)}}
th{{color:var(--muted);font-weight:600;white-space:nowrap}}
pre{{background:var(--panel);padding:12px 14px;overflow-x:auto;white-space:pre-wrap;word-break:break-word;margin:6px 0 14px}}
details{{border-top:1px solid var(--rule);padding:6px 0}}
summary{{cursor:pointer}}
li{{margin:4px 0}}
.shots{{display:grid;gap:10px;align-items:start;margin:12px 0;grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr))}}
.shots figure{{margin:0}}.shots img{{width:100%;border:1px solid var(--rule)}}
figcaption{{font:11px ui-monospace,Menlo,monospace;color:var(--muted)}}
</style></head><body><main>
<h1>{html.escape(title)}</h1>
<div class="meta">{html.escape(meta)}</div>
{section("1. Summary per acceptance criterion", note("summary.md"))}
{section("2. Decisions for the maintainer", note("decisions.md"))}
{section("3. Scenario results", scenarios)}
{section("4. Not verified", note("not-verified.md"))}
{section("5. Environment", note("environment.md"))}
</main></body></html>
"""
(root / "report.html").write_text(page)
print(root / "report.html")
