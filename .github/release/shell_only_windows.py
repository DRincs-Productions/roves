"""Turn a tested Windows player package into the release's shell-only package.

The Windows workflow packages and tests the player with the visual-novel-template sample
game. The release ships the shell only, so this removes game/, the sample game's window
icon (icon.png) and its provenance files, rewrites PROVENANCE.txt without the sample game
and regenerates SHA256SUMS.txt (same format as the workflow: "<sha256>  <path>", sorted by
path, forward slashes). Usage: python3 -I shell_only_windows.py <package folder>
"""

import hashlib
import shutil
import sys
from pathlib import Path

SHELL_NOTE = """Shell only
  The release package holds the player without a game: the game folder next to play.exe
  (index.html at its root) is added afterwards by Roves Packmaster, roves-action or by
  hand. Before publishing, the same player was tested with a sample game.
"""


def strip_provenance(text: str) -> str:
    out, skipping = [], False
    lines = text.splitlines()
    if lines and lines[0].startswith('visual-novel-template on the Roves'):
        title = 'Roves CEF + SDL3 + wgpu player shell (Windows x64)'
        lines[0:2] = [title, '=' * len(title)]
    for line in lines:
        if line.startswith('Game (folder game'):
            skipping = True
            continue
        if skipping:
            if line.strip() and not line.startswith(' '):
                skipping = False
            else:
                continue
        if 'visual-novel-template' in line:
            continue
        line = line.replace('the\n          game in the game folder', 'the game folder')
        out.append(line)
    text = '\n'.join(out).rstrip('\n') + '\n\n' + SHELL_NOTE
    assert 'visual-novel-template' not in text
    return text


def main() -> None:
    pkg = Path(sys.argv[1])
    shutil.rmtree(pkg / 'game', ignore_errors=True)
    (pkg / 'icon.png').unlink(missing_ok=True)
    for f in (pkg / 'provenance').glob('visual-novel-template-*'):
        f.unlink()
    prov = pkg / 'PROVENANCE.txt'
    prov.write_text(strip_provenance(prov.read_text(encoding='utf-8-sig')), encoding='utf-8')
    lines = []
    for f in pkg.rglob('*'):
        if f.is_file() and f.name != 'SHA256SUMS.txt':
            rel = f.relative_to(pkg).as_posix()
            lines.append((rel, hashlib.sha256(f.read_bytes()).hexdigest()))
    lines.sort()
    (pkg / 'SHA256SUMS.txt').write_text(''.join(f'{d}  {r}\n' for r, d in lines), encoding='utf-8')
    print(f'{pkg.name}: shell only, {len(lines)} files listed')


if __name__ == '__main__':
    main()
