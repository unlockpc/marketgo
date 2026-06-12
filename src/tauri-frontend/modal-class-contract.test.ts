import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

// Regression guard for: "点击快速养号没反应".
// Root cause was `modal.classList.add('show')` while the only modal-visibility
// CSS rule is `.modal.active { display: flex }` — adding an unknown class left
// the modal at `display:none`, so it silently never appeared.
//
// Contract: any class added to a `modal` element in the frontend must be backed
// by a matching `.modal.<class>` rule in the stylesheet.

const here = dirname(fileURLToPath(import.meta.url));
const appTs = readFileSync(resolve(here, 'app.ts'), 'utf8');
const css = readFileSync(resolve(here, '../../dist/tauri/styles/main.css'), 'utf8');

/** Classes that the stylesheet recognizes as `.modal.<class>` (visibility toggles). */
function cssModalClasses(src: string): Set<string> {
  const set = new Set<string>();
  const re = /\.modal\.([a-zA-Z0-9_-]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src))) set.add(m[1]);
  return set;
}

/** Classes added to a `modal` element via `modal.classList.add('<class>')`. */
function tsModalAddClasses(src: string): string[] {
  const out: string[] = [];
  const re = /\bmodal\.classList\.add\(\s*['"]([a-zA-Z0-9_-]+)['"]\s*\)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src))) out.push(m[1]);
  return out;
}

describe('modal open CSS class contract', () => {
  it('detector flags an orphan class (self-check)', () => {
    // Proves the test would have caught the original `show` bug.
    const buggy = `const modal = x; modal.classList.add('show');`;
    const cssClasses = new Set(['active']);
    const orphans = tsModalAddClasses(buggy).filter((c) => !cssClasses.has(c));
    expect(orphans).toEqual(['show']);
  });

  it('every class added to a modal element is backed by a .modal.<class> CSS rule', () => {
    const cssClasses = cssModalClasses(css);
    expect(cssClasses.size).toBeGreaterThan(0); // sanity: stylesheet parsed

    const added = [...new Set(tsModalAddClasses(appTs))];
    expect(added.length).toBeGreaterThan(0); // sanity: found the open() calls

    const orphans = added.filter((c) => !cssClasses.has(c));
    expect(
      orphans,
      `modal classes with no matching ".modal.<class>" CSS rule (modal will stay display:none): ${orphans.join(', ')}`,
    ).toEqual([]);
  });
});
