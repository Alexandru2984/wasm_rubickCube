#!/usr/bin/env python3
"""Post-processes Trunk's dist/ for a strict CSP and long-lived caching.

1. ui/ → ui-<content hash>/ (immutable URL), with SRI on the entry
   stylesheet/script and SRI'd modulepreloads for every UI module.
2. Trunk's inline bootstrap <script type="module"> moves to a hashed external
   file, so the CSP needs no 'unsafe-inline'. While at it, the WASM size Trunk
   bakes in for the progress bar is corrected (Trunk records the size before
   wasm-opt, so the bar used to stop at ~68%).
3. Fails the build if any inline script, inline event handler or style
   attribute is left in index.html.
4. Injects the precache list and cache version into service-worker.js.
5. Points 404.html at the same hashed stylesheet and writes sitemap.xml.
6. Writes .gz siblings for nginx gzip_static (reproducible: mtime 0), and
   .br siblings (brotli, quality 11) that the vhost hands to clients that
   take brotli: ~30% smaller than gzip on the WASM. The brotli CLI is
   optional; without it the .br files are skipped with a warning and nginx
   falls back to gzip.

Standard library (plus the brotli CLI); run as: python3 tools/postbuild.py dist
"""

import base64
import gzip
import hashlib
import json
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SITE_URL = "https://cube.micutu.com/"
COMPRESSIBLE = {".wasm", ".js", ".mjs", ".css", ".svg", ".html", ".webmanifest", ".json", ".txt", ".xml"}
NO_PRECACHE = {"index.html", "service-worker.js", "robots.txt", "sitemap.xml", "404.html", "humans.txt", "og.jpg"}


def sri(data: bytes) -> str:
    return "sha384-" + base64.b64encode(hashlib.sha384(data).digest()).decode()


def fail(msg: str) -> None:
    print(f"postbuild: {msg}", file=sys.stderr)
    sys.exit(1)


def rewrite_ui_refs(html: str, tag: str, target: Path, page: str) -> str:
    """Points /ui/ stylesheet and module links at the hashed dir, with SRI."""

    def rewrite(match: re.Match) -> str:
        prefix, rel = match.group(1), match.group(2)
        path = target / rel
        if not path.is_file():
            fail(f"{page} references missing ui/{rel}")
        return f'{prefix}"/{tag}/{rel}" integrity="{sri(path.read_bytes())}"'

    html = re.sub(r'(<link rel="stylesheet" href=)"/ui/([^"]+)"', rewrite, html)
    html = re.sub(r'(<script type="module" src=)"/ui/([^"]+)"', rewrite, html)
    if "/ui/" in re.sub(r"<!--.*?-->", "", html, flags=re.S):
        fail(f"unrewritten /ui/ reference left in {page}")
    return html


def hash_ui(dist: Path, html: str) -> tuple[str, str]:
    ui = dist / "ui"
    if not ui.is_dir():
        fail("dist/ui missing (is the copy-dir link in index.html?)")
    h = hashlib.sha256()
    files = sorted(f for f in ui.rglob("*") if f.is_file())
    for f in files:
        h.update(f.relative_to(ui).as_posix().encode())
        h.update(b"\0")
        h.update(f.read_bytes())
    tag = f"ui-{h.hexdigest()[:12]}"
    target = dist / tag
    if target.exists():
        shutil.rmtree(target)
    ui.rename(target)

    html = rewrite_ui_refs(html, tag, target, "index.html")
    preloads = "".join(
        f'\n    <link rel="modulepreload" href="/{tag}/{f.name}" integrity="{sri(f.read_bytes())}" />'
        for f in sorted(target.glob("*.js"))
        if f.name != "app.js"
    )
    html = html.replace("</head>", f"{preloads}\n  </head>", 1)
    return html, tag


def externalize_boot(dist: Path, html: str) -> str:
    pattern = re.compile(r"<script type=\"module\">(?P<body>.*?)</script>", re.S)
    boots = [m for m in pattern.finditer(html) if "__trunkInitializer" in m.group("body") or "wasmBindings" in m.group("body")]
    if len(boots) != 1:
        fail(f"expected exactly one Trunk bootstrap script, found {len(boots)}")
    m = boots[0]
    body = m.group("body")

    def fix_size(sm: re.Match) -> str:
        wasm = dist / sm.group(2).lstrip("/")
        if not wasm.is_file():
            fail(f"bootstrap references missing {wasm.name}")
        return f"{sm.group(1)}{wasm.stat().st_size}{sm.group(4)}"

    body, n = re.subn(r"(__trunkInitializer\(init, '(/[^']+_bg\.wasm)', )(\d+)(,)", fix_size, body)
    if n != 1:
        fail("couldn't find the WASM size in the bootstrap script")
    data = body.strip().encode() + b"\n"
    name = f"boot-{hashlib.sha256(data).hexdigest()[:16]}.mjs"
    (dist / name).write_bytes(data)
    tag = f'<script type="module" src="/{name}" integrity="{sri(data)}" crossorigin="anonymous"></script>'
    return html[: m.start()] + tag + html[m.end() :]


def check_csp_clean(html: str) -> None:
    stripped = re.sub(r"<!--.*?-->", "", html, flags=re.S)
    for m in re.finditer(r"<script(?P<attrs>[^>]*)>(?P<body>.*?)</script>", stripped, re.S):
        attrs = m.group("attrs")
        if "src=" not in attrs and 'type="application/ld+json"' not in attrs and m.group("body").strip():
            fail("inline <script> left in index.html (CSP would block it)")
    if re.search(r"\son[a-z]+\s*=", stripped):
        fail("inline event handler attribute in index.html")
    if re.search(r"\sstyle\s*=", stripped):
        fail("style attribute in index.html (CSP style-src has no 'unsafe-inline')")


def write_sitemap(dist: Path) -> None:
    """One URL (the app routes with #fragments); lastmod = build day."""
    today = datetime.now(timezone.utc).date().isoformat()
    (dist / "sitemap.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        f"  <url><loc>{SITE_URL}</loc><lastmod>{today}</lastmod></url>\n"
        "</urlset>\n"
    )


def write_sw(dist: Path) -> None:
    sw = dist / "service-worker.js"
    files = sorted(
        f
        for f in dist.rglob("*")
        if f.is_file() and f.suffix not in (".gz", ".br") and f.relative_to(dist).as_posix() not in NO_PRECACHE
    )
    urls = ["/" + f.relative_to(dist).as_posix() for f in files]
    h = hashlib.sha256()
    for f in files:
        h.update(f.relative_to(dist).as_posix().encode())
        h.update(hashlib.sha256(f.read_bytes()).digest())
    version = h.hexdigest()[:16]
    src = sw.read_text()
    if "'__CACHE_VERSION__'" not in src or "__PRECACHE__" not in src:
        fail("service-worker.js placeholders missing")
    src = src.replace("'__CACHE_VERSION__'", json.dumps(version)).replace("__PRECACHE__", json.dumps(urls, indent=0))
    sw.write_text(src)
    print(f"postbuild: service worker version {version}, {len(urls)} precached files")


def precompress(dist: Path) -> None:
    brotli = shutil.which("brotli")
    if not brotli:
        print("postbuild: warning: no brotli CLI, skipping .br files (apt install brotli)")
    files = [f for f in dist.rglob("*") if f.is_file() and f.suffix in COMPRESSIBLE and f.stat().st_size >= 512]
    raw = br = 0
    for f in files:
        data = f.read_bytes()
        gz = Path(str(f) + ".gz")
        gz.write_bytes(gzip.compress(data, compresslevel=9, mtime=0))
        if brotli:
            out = Path(str(f) + ".br")
            subprocess.run([brotli, "-q", "11", "-f", "-o", str(out), str(f)], check=True)
            if subprocess.run([brotli, "-d", "-c", str(out)], check=True, capture_output=True).stdout != data:
                fail(f"brotli round trip failed for {f.name}")
            raw += len(data)
            br += out.stat().st_size
    if brotli:
        print(f"postbuild: brotli {raw} -> {br} bytes over {len(files)} files")


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: postbuild.py <dist-dir>")
    dist = Path(sys.argv[1])
    index = dist / "index.html"
    html = index.read_text()
    for stale in [*dist.rglob("*.gz"), *dist.rglob("*.br")]:
        stale.unlink()
    html, tag = hash_ui(dist, html)
    # Chrome ignores (and warns about) SRI on as="fetch" preloads; the WASM
    # is fetched by the bootstrap, whose own integrity is pinned.
    html = re.sub(r'(<link rel="preload"[^>]*?) integrity="[^"]*"([^>]*as="fetch")', r"\1\2", html)
    html = externalize_boot(dist, html)
    check_csp_clean(html)
    index.write_text(html)
    not_found = dist / "404.html"
    if not_found.is_file():
        page = rewrite_ui_refs(not_found.read_text(), tag, dist / tag, "404.html")
        check_csp_clean(page)
        not_found.write_text(page)
    write_sitemap(dist)
    write_sw(dist)
    precompress(dist)
    print("postbuild: ok")


if __name__ == "__main__":
    main()
