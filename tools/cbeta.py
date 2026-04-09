"""CBETA plugin — incremental ingestion of the Chinese Buddhist Canon (大藏經).

Fetches sutra texts from the CBETA GitHub XML repository and the CBETA API.
Designed for progressive learning: each run picks a batch of unread texts,
ingests and compiles them. Over time, the entire Tripiṭaka gets absorbed.
"""

import json
import re
import time
from datetime import datetime, timezone
from pathlib import Path

import frontmatter
import requests
from bs4 import BeautifulSoup

from .config import load_config, ensure_dirs

CBETA_API = "https://cbdata.dila.edu.tw/stable"
CBETA_XML = "https://raw.githubusercontent.com/cbeta-org/xml-p5/master"
GITHUB_API = "https://api.github.com/repos/cbeta-org/xml-p5/contents"
HEADERS = {"Referer": "https://llmbase.dev"}

# CBETA category structure
CATEGORIES = {
    "agama": ("CBETA.001", "阿含部"),
    "benyuan": ("CBETA.002", "本缘部"),
    "bore": ("CBETA.003", "般若部"),
    "fahua": ("CBETA.004", "法华部"),
    "huayan": ("CBETA.005", "华严部"),
    "baoji": ("CBETA.006", "宝积部"),
    "niepan": ("CBETA.007", "涅槃部"),
    "daji": ("CBETA.008", "大集部"),
    "jingji": ("CBETA.009", "经集部"),
    "mijiao": ("CBETA.010", "密教部"),
    "lv": ("CBETA.011", "律部"),
    "pitan": ("CBETA.012", "毗昙部"),
    "zhongguan": ("CBETA.013", "中观部"),
    "yuqie": ("CBETA.014", "瑜伽部"),
    "lunji": ("CBETA.015", "论集部"),
    "jingtu": ("CBETA.016", "净土宗部"),
    "chanzong": ("CBETA.017", "禅宗部"),
    "shizhuan": ("CBETA.018", "史传部"),
}

# Map canon codes to Chinese names
CANON_NAMES = {
    "T": "大正藏", "X": "卍续藏", "J": "嘉兴藏", "N": "南传大藏经",
    "B": "大藏经补编", "Y": "印顺法师著作集", "LC": "吕澂著作集",
}


def get_progress_file(base_dir: Path) -> Path:
    """Get path to the progress tracking file."""
    meta_dir = Path(load_config(base_dir)["paths"]["meta"])
    meta_dir.mkdir(parents=True, exist_ok=True)
    return meta_dir / "cbeta_progress.json"


def load_progress(base_dir: Path) -> dict:
    """Load ingestion progress."""
    pf = get_progress_file(base_dir)
    if pf.exists():
        return json.loads(pf.read_text())
    return {"ingested_works": [], "total_ingested": 0, "last_run": None}


def save_progress(base_dir: Path, progress: dict):
    """Save ingestion progress."""
    pf = get_progress_file(base_dir)
    pf.write_text(json.dumps(progress, indent=2, ensure_ascii=False))


# ─── Catalog API ─────────────────────────────────────────────

def list_categories() -> list[dict]:
    """List all CBETA categories."""
    resp = requests.get(f"{CBETA_API}/catalog_entry", params={"q": "CBETA"}, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    data = resp.json()
    results = data.get("results", data) if isinstance(data, dict) else data
    return [{"id": e.get("n"), "label": e.get("label", "")} for e in results]


def list_works_in_category(category_id: str) -> list[dict]:
    """Recursively list all individual works (sutras) in a category."""
    works = []
    _collect_works(category_id, works)
    return works


def _collect_works(node_id: str, works: list, depth: int = 0):
    """Recursively traverse catalog tree to find leaf works."""
    if depth > 5:
        return
    resp = requests.get(f"{CBETA_API}/catalog_entry", params={"q": node_id}, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    data = resp.json()
    results = data.get("results", data) if isinstance(data, dict) else data
    if not isinstance(results, list):
        return

    for entry in results:
        work_id = entry.get("work", "")
        if work_id:
            works.append({
                "work": work_id,
                "label": entry.get("label", ""),
                "category": entry.get("category", ""),
                "creator": entry.get("creator", ""),
            })
        else:
            child_id = entry.get("n", "")
            if child_id and child_id != node_id:
                _collect_works(child_id, works, depth + 1)
    time.sleep(0.5)


# ─── XML Text Fetching ──────────────────────────────────────

def fetch_sutra_xml(work_id: str) -> str:
    """Fetch sutra XML from CBETA GitHub repository.

    work_id format: T0001, X1456, etc.
    """
    canon = re.match(r"([A-Z]+)", work_id)
    if not canon:
        raise ValueError(f"Invalid work_id: {work_id}")
    canon_code = canon.group(1)

    # Find volume number from work_id (e.g. T0001 -> T01)
    num = re.search(r"\d+", work_id)
    if not num:
        raise ValueError(f"No number in work_id: {work_id}")

    # List files in the canon directory to find matching files
    vol_dirs = _find_volume_dir(canon_code, work_id)
    if not vol_dirs:
        raise FileNotFoundError(f"Cannot find XML for {work_id}")

    # Fetch all matching XML files and combine
    combined_text = ""
    for vol_dir, filename in vol_dirs:
        url = f"{CBETA_XML}/{canon_code}/{vol_dir}/{filename}"
        resp = requests.get(url, timeout=30)
        if resp.status_code == 200:
            text = _parse_xml_to_text(resp.text, work_id)
            combined_text += text + "\n\n"

    return combined_text.strip()


def _find_volume_dir(canon_code: str, work_id: str) -> list[tuple[str, str]]:
    """Find the volume directory and filenames for a work."""
    # Normalize work_id: T0001 -> T01n0001
    num_match = re.search(r"(\d+)", work_id)
    if not num_match:
        return []
    work_num = num_match.group(1)
    padded = work_num.zfill(4)

    # Try common volume patterns
    for vol_num in range(1, 100):
        vol_dir = f"{canon_code}{str(vol_num).zfill(2)}"
        filename = f"{vol_dir}n{padded}.xml"
        url = f"{CBETA_XML}/{canon_code}/{vol_dir}/{filename}"
        resp = requests.head(url, timeout=10)
        if resp.status_code == 200:
            return [(vol_dir, filename)]

    # Fallback: search via GitHub API
    try:
        resp = requests.get(f"{GITHUB_API}/{canon_code}", timeout=15)
        if resp.status_code == 200:
            dirs = [d["name"] for d in resp.json() if d["type"] == "dir"]
            for d in dirs:
                resp2 = requests.get(f"{GITHUB_API}/{canon_code}/{d}", timeout=15)
                if resp2.status_code == 200:
                    files = [f["name"] for f in resp2.json() if f["name"].startswith(f"{d}n{padded}")]
                    if files:
                        return [(d, f) for f in files]
    except Exception:
        pass

    return []


def _parse_xml_to_text(xml_str: str, work_id: str) -> str:
    """Parse CBETA XML to plain text."""
    soup = BeautifulSoup(xml_str, features="html.parser")

    # Remove notes, apparatus, rdg (variant readings)
    for tag in soup.find_all(["note", "rdg", "app", "anchor"]):
        tag.decompose()

    body = soup.find("body")
    if not body:
        return ""

    text = body.get_text(separator="\n", strip=True)

    # Clean up: remove excessive whitespace, keep paragraph structure
    lines = [line.strip() for line in text.split("\n") if line.strip()]
    return "\n\n".join(lines)


# ─── Ingestion ───────────────────────────────────────────────

def ingest_work(
    work_id: str,
    label: str = "",
    creator: str = "",
    base_dir: Path | None = None,
) -> Path | None:
    """Ingest a single CBETA work into the knowledge base."""
    cfg = load_config(base_dir)
    ensure_dirs(cfg)
    raw_dir = Path(cfg["paths"]["raw"])

    # Check if already ingested
    slug = f"cbeta-{work_id.lower()}"
    doc_dir = raw_dir / slug
    if (doc_dir / "index.md").exists():
        return None  # Already ingested

    # Fetch text
    try:
        text = fetch_sutra_xml(work_id)
    except Exception as e:
        return None

    if not text or len(text) < 50:
        return None

    # Determine title
    canon_code = re.match(r"[A-Z]+", work_id).group(0)
    canon_name = CANON_NAMES.get(canon_code, canon_code)
    title = label or f"{canon_name} {work_id}"
    # Clean label (remove work_id prefix if present)
    if title.startswith(work_id):
        title = title[len(work_id):].strip()
    if not title:
        title = f"{canon_name} {work_id}"

    doc_dir.mkdir(parents=True, exist_ok=True)
    post = frontmatter.Post(text)
    post.metadata["title"] = title
    post.metadata["source"] = f"https://cbetaonline.dila.edu.tw/zh/{work_id}"
    post.metadata["ingested_at"] = datetime.now(timezone.utc).isoformat()
    post.metadata["type"] = "buddhist_sutra"
    post.metadata["work_id"] = work_id
    post.metadata["canon"] = canon_name
    post.metadata["creator"] = creator
    post.metadata["compiled"] = False

    doc_path = doc_dir / "index.md"
    doc_path.write_text(frontmatter.dumps(post), encoding="utf-8")
    return doc_path


def learn(
    category: str | None = None,
    batch_size: int = 5,
    base_dir: Path | None = None,
) -> list[str]:
    """Progressive learning: ingest a batch of unread sutras.

    Each call picks the next `batch_size` works not yet ingested.
    Call repeatedly to incrementally absorb the entire canon.

    Local progress is validated against on-disk raw doc presence so a
    volume wipe cannot leave stale entries in the skip-set. After each
    successful ingest an ``ingested`` hook is emitted — downstream can
    register callbacks (e.g. remote sync) via ``tools.hooks.register``.
    """
    from .hooks import emit

    base = Path(base_dir) if base_dir else Path.cwd()
    cfg = load_config(base)
    raw_dir = Path(cfg["paths"]["raw"])
    progress = load_progress(base)
    ingested_set = set(progress["ingested_works"])

    def _raw_exists(work_id: str) -> bool:
        return (raw_dir / f"cbeta-{work_id.lower()}" / "index.md").exists()

    # Validate local progress against disk — volume reset / partial wipes
    # may leave stale progress entries pointing at files that no longer exist.
    stale_local = {w for w in ingested_set if not _raw_exists(w)}
    if stale_local:
        ingested_set -= stale_local
        progress["ingested_works"] = sorted(ingested_set)
        progress["total_ingested"] = len(ingested_set)
        save_progress(base, progress)

    # Get works to process
    if category:
        cat_id, cat_name = CATEGORIES.get(category, (None, None))
        if not cat_id:
            raise ValueError(f"Unknown category: {category}. Available: {list(CATEGORIES.keys())}")
        works = list_works_in_category(cat_id)
    else:
        # Default: go through categories in order
        works = []
        for cat_key, (cat_id, cat_name) in CATEGORIES.items():
            cat_works = list_works_in_category(cat_id)
            works.extend(cat_works)
            if len([w for w in works if w["work"] not in ingested_set]) >= batch_size * 2:
                break  # Enough candidates

    # Filter out already ingested
    pending = [w for w in works if w["work"] not in ingested_set]

    if not pending:
        return []

    # Process batch
    batch = pending[:batch_size]
    results = []

    for work_info in batch:
        work_id = work_info["work"]
        label = work_info.get("label", "")
        creator = work_info.get("creator", "")

        path = ingest_work(work_id, label, creator, base)
        if path:
            results.append(work_id)
            ingested_set.add(work_id)
            emit("ingested", source="cbeta", work_id=work_id, title=label)

        time.sleep(1)  # Be respectful to GitHub

    # Save progress
    progress["ingested_works"] = sorted(ingested_set)
    progress["total_ingested"] = len(ingested_set)
    progress["last_run"] = datetime.now(timezone.utc).isoformat()
    save_progress(base, progress)

    return results


def status(base_dir: Path | None = None) -> dict:
    """Get current learning progress."""
    base = Path(base_dir) if base_dir else Path.cwd()
    progress = load_progress(base)
    return {
        "total_ingested": progress["total_ingested"],
        "last_run": progress.get("last_run"),
        "ingested_works": progress["ingested_works"][:20],  # Show recent
    }
