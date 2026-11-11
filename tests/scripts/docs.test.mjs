import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import test from 'node:test';

import { prepare, sha256 } from '../../scripts/docs.mjs';

void test('offline pack preserves code and indexes sections without navigation', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-docs-test-'));
  try {
    const source = path.join(root, 'source');
    const htmlRoot = path.join(source, 'share', 'doc', 'lilypond', 'html');
    fs.mkdirSync(path.join(htmlRoot, 'Documentation', 'notation'), { recursive: true });
    fs.mkdirSync(path.join(htmlRoot, 'Documentation', 'images'), { recursive: true });
    fs.writeFileSync(
      path.join(htmlRoot, 'Documentation', 'notation', 'pitches.html'),
      `<div id="main"><div class="subsection" id="relative-octave-entry">
<table class="nav_table"><tr><td>Do not index navigation</td></tr></table>
<h4 class="subsection">Relative octave entry</h4><p>Use relative octave input.</p>
<pre>\\relative c' {\n  c4 d e f |\n}</pre><img src="../images/notes.png">
<h4 class="subheading">See also</h4><p>More examples.</p></div></div>
<div id="footer">Do not index footer</div>`,
    );
    fs.writeFileSync(path.join(htmlRoot, 'Documentation', 'images', 'notes.png'), 'image');
    fs.writeFileSync(path.join(htmlRoot, 'Documentation', 'notation', 'pitches.fr.html'), 'French');
    fs.writeFileSync(path.join(htmlRoot, 'COPYING.FDL'), 'Original license');
    fs.writeFileSync(path.join(htmlRoot, 'notation.pdf'), 'PDF');
    const archive = path.join(root, 'docs.tar.xz');
    execFileSync('tar', ['-cJf', archive, '-C', source, '.'], {
      env: { ...process.env, XZ_OPT: '-1' },
    });
    const lock = {
      version: '2.26.0',
      formatVersion: 1,
      archiveRoot: 'share/doc/lilypond/html',
      manuals: ['notation'],
      sha256: sha256(archive),
    };
    const output = path.join(root, 'pack');
    const result = prepare(archive, output, lock);
    assert.equal(result.chapters, 1);
    const database = new DatabaseSync(path.join(output, 'index.sqlite'));
    const chapter = database.prepare('SELECT id, body FROM chapters').get();
    assert.equal(chapter.id, 'notation/pitches#relative-octave-entry');
    assert.match(chapter.body, /\\relative c' \{\n  c4 d e f \|\n\}/);
    assert.doesNotMatch(chapter.body, /Do not index/);
    assert.equal(
      database.prepare("SELECT count(*) count FROM search WHERE search MATCH 'relative'").get()
        .count,
      1,
    );
    database.close();
    const members = execFileSync('tar', ['-tf', path.join(output, 'lilypond-2.26.0-en.tar.xz')], {
      encoding: 'utf8',
    })
      .split('\n')
      .filter(Boolean);
    assert.ok(members.some((name) => name.endsWith('Documentation/images/notes.png')));
    assert.equal(
      members.some((name) => name.endsWith('.pdf')),
      false,
    );
    lock.sha256 = '0'.repeat(64);
    assert.throws(() => prepare(archive, output, lock), /checksum/);
    assert.equal(
      JSON.parse(fs.readFileSync(path.join(output, 'manifest.json'), 'utf8')).contentHash,
      result.contentHash,
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
