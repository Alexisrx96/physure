"""Remove DA:N,0 entries for lines that are only macro attributes (#[pymethods], etc.)."""

import re
import sys

ATTR_PATTERN = re.compile(
    r"^#\[(pymethods|pyclass|pyfn|pyfunction|new|getter|setter|staticmethod)\]$"
)


def attr_only_lines(src_path: str) -> set[int]:
    try:
        with open(src_path) as f:
            return {
                i
                for i, ln in enumerate(f, 1)
                if ATTR_PATTERN.match(ln.strip())
            }
    except FileNotFoundError:
        return set()


def filter_block(block: str) -> str:
    sf = re.search(r"^SF:(.+)$", block, re.MULTILINE)
    if not sf:
        return block
    skip = attr_only_lines(sf.group(1))
    if not skip:
        return block
    return re.sub(
        r"^DA:(\d+),0(,\S+)?$",
        lambda m: "" if int(m.group(1)) in skip else m.group(0),
        block,
        flags=re.MULTILINE,
    )


def main(lcov_file: str) -> None:
    with open(lcov_file) as f:
        content = f.read()
    blocks = content.split("end_of_record")
    filtered = "end_of_record".join(filter_block(b) for b in blocks)
    # Collapse blank lines left by removed DA entries
    filtered = re.sub(r"\n{3,}", "\n\n", filtered)
    with open(lcov_file, "w") as f:
        f.write(filtered)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "lcov.info")
