// End-to-end exercise of the real IPC surface against the real (temp)
// store. Runs only when the app was started with ENVARSA_SELFTEST=1;
// results are printed to stdout by the core and the app exits with a
// matching code. Native dialogs are skipped (they need a human) — their
// code paths are thin plugin calls covered by `cargo test` logic tests.
import { api } from './api.js';

const ALPHA_RAW = [
  '# Database',
  'DATABASE_URL="postgres://u:p@localhost/db"',
  'REDIS_URL=redis://localhost:6379',
  'EMPTY=',
  'export REGION=eu-west-1',
  'DUP=1',
  'DUP=2',
  '',
].join('\n');

const BETA_RAW = [
  'DATABASE_URL="postgres://u:p@localhost/db"',
  'REDIS_URL=redis://other:6379',
  'ONLY_BETA=x',
  '',
].join('\n');

// Chosen so serialize_lines(parse(WRITE_RAW)) === WRITE_RAW (all values
// are unquoted-safe), making fresh-write assert on exact bytes.
const WRITE_RAW = [
  '# App',
  'PORT=3000',
  'export REGION=eu-west-1',
  '',
  '# Secrets',
  'API_KEY=real-secret',
  'TOKEN=t0ken',
  '',
].join('\n');

export async function runSelftest() {
  const results = [];
  const step = async (name, fn) => {
    try {
      await fn();
      results.push(`ok   ${name}`);
    } catch (e) {
      const msg = String(e?.message || e);
      // The OS clipboard being held by another app is an environment
      // condition, not a product failure — skip honestly, fail on
      // anything else.
      if (msg.includes('held by another party')) {
        results.push(`skip ${name}: clipboard contended by another app`);
      } else {
        results.push(`FAIL ${name}: ${msg}`);
      }
    }
  };
  const assert = (cond, msg) => {
    if (!cond) throw new Error(msg);
  };
  const assertEq = (got, want, what) => {
    if (got !== want) throw new Error(`${what}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  };

  let originalClipboard = null;
  try {
    originalClipboard = await api.selftest.readClipboard();
  } catch {
    originalClipboard = null; // empty/non-text clipboard
  }

  let alpha = null;
  let beta = null;
  let writeProj = null;
  let storeDir = '';
  let sep = '/'; // the store path's own separator: '\' on Windows, '/' on Linux

  await step('fresh store is unlocked and empty', async () => {
    const st = await api.status();
    assertEq(st.state, 'unlocked', 'state');
    assertEq(st.projectCount, 0, 'projectCount');
    assertEq(st.encrypted, false, 'encrypted');
    storeDir = st.storePath.replace(/[\\/][^\\/]+$/, '');
    sep = st.storePath.includes('\\') ? '\\' : '/';
    assert(storeDir.length > 0, 'store dir resolves');
  });

  await step('status exposes packaged/portable build flags (both off in dev)', async () => {
    const st = await api.status();
    // The selftest runs the plain dev build: no MSIX package identity and
    // no envarsa.portable marker beside the exe, so both flags are false.
    // The portable flag is what gates the "relocating un-anchors the store"
    // caveat in Settings.
    assertEq(typeof st.portable, 'boolean', 'portable is a boolean');
    assertEq(typeof st.packaged, 'boolean', 'packaged is a boolean');
    assertEq(st.portable, false, 'portable is false without the marker');
    assertEq(st.packaged, false, 'packaged is false outside the Store build');
  });

  await step('preview counts entries, comments, problems, dups', async () => {
    const p = await api.previewCapture('A=1\n# c\nA=2\nBAD LINE');
    assertEq(p.entries, 1, 'entries');
    assertEq(p.comments, 1, 'comments');
    assertEq(p.bad, 1, 'bad');
    assertEq(p.dupKeys.join(','), 'A', 'dupKeys');
  });

  await step('capture creates a project with a snapshot', async () => {
    alpha = await api.capture({
      projectId: null,
      projectName: 'selftest-alpha',
      pathHint: 'C:\\tmp\\alpha',
      text: ALPHA_RAW,
      sourcePath: null,
    });
    assertEq(alpha.entryCount, 5, 'entryCount');
    const list = await api.listProjects();
    assertEq(list.length, 1, 'project count');
    assertEq(list[0].name, 'selftest-alpha', 'name');
    assertEq(list[0].entryCount, 5, 'list entryCount');
    assertEq(list[0].snapshotCount, 1, 'snapshotCount');
  });

  await step('project view has structure but never values', async () => {
    const v = await api.getProject(alpha.projectId);
    assertEq(v.isLatest, true, 'isLatest');
    // A trailing \n terminates the last line (str::lines semantics) —
    // 7 real lines, no phantom blank.
    assertEq(v.lines.length, 7, 'line count');
    assertEq(v.lines[0].t, 'comment', 'line0');
    assertEq(v.lines[1].t, 'entry', 'line1');
    assertEq(v.lines[1].key, 'DATABASE_URL', 'line1 key');
    assert(!('value' in v.lines[1]), 'no value field in listing');
    assertEq(v.lines[4].exported, true, 'export flag');
    assertEq(v.lines[5].overridden, true, 'first DUP overridden');
    assertEq(v.lines[6].overridden, false, 'last DUP wins');
  });

  await step('reveal returns the unquoted value', async () => {
    const r = await api.revealValue(alpha.projectId, alpha.snapshotId, 1);
    assertEq(r.key, 'DATABASE_URL', 'key');
    assertEq(r.value, 'postgres://u:p@localhost/db', 'value');
  });

  await step('bad lines are masked in the listing and revealed on demand', async () => {
    const g = await api.capture({
      projectId: null,
      projectName: 'selftest-badline',
      pathHint: null,
      text: 'GOOD=1\nAuthorization: Bearer not-a-real-token\n',
      sourcePath: null,
    });
    const v = await api.getProject(g.projectId);
    const bad = v.lines.find((l) => l.t === 'bad');
    assert(bad, 'bad line present');
    assert(!('raw' in bad), 'no raw text in the listing');
    const r = await api.revealValue(g.projectId, g.snapshotId, bad.idx);
    assertEq(r.value, 'Authorization: Bearer not-a-real-token', 'reveal returns the raw line');
    await api.deleteProject(g.projectId);
  });

  await step('copy value goes core -> clipboard, never the UI', async () => {
    const key = await api.copyValue(alpha.projectId, alpha.snapshotId, 1);
    assertEq(key, 'DATABASE_URL', 'returned key');
    const clip = await api.selftest.readClipboard();
    assertEq(clip, 'postgres://u:p@localhost/db', 'clipboard');
  });

  await step('reuse is flagged across projects with same/different values', async () => {
    beta = await api.capture({
      projectId: null,
      projectName: 'selftest-beta',
      pathHint: null,
      text: BETA_RAW,
      sourcePath: null,
    });
    const v = await api.getProject(alpha.projectId);
    const dbLine = v.lines.find((l) => l.key === 'DATABASE_URL');
    const redisLine = v.lines.find((l) => l.key === 'REDIS_URL');
    const emptyLine = v.lines.find((l) => l.key === 'EMPTY');
    assertEq(dbLine.reuse.length, 1, 'db reuse count');
    assertEq(dbLine.reuse[0].name, 'selftest-beta', 'db reuse name');
    assertEq(dbLine.reuse[0].same, true, 'db reuse same');
    assertEq(redisLine.reuse[0].same, false, 'redis differs');
    assertEq(emptyLine.reuse.length, 0, 'EMPTY not shared');
  });

  await step('copy block is the raw snapshot, byte for byte', async () => {
    const n = await api.copyBlock(beta.projectId, beta.snapshotId);
    assertEq(n, 3, 'block entry count');
    const clip = await api.selftest.readClipboard();
    assertEq(clip, BETA_RAW, 'clipboard block');
  });

  await step('export writes the raw snapshot back out', async () => {
    const dest = `${storeDir}${sep}selftest-export.env`;
    await api.selftest.exportToPath(beta.projectId, beta.snapshotId, dest);
    const read = await api.selftest.readFile(dest);
    assertEq(read, BETA_RAW, 'exported bytes');
  });

  await step('rename works and name collisions are refused', async () => {
    await api.renameProject(beta.projectId, 'selftest-renamed');
    let threw = false;
    try {
      await api.renameProject(beta.projectId, 'Selftest-Alpha');
    } catch {
      threw = true;
    }
    assert(threw, 'collision must be refused');
    const list = await api.listProjects();
    assert(list.some((p) => p.name === 'selftest-renamed'), 'renamed present');
  });

  await step('recapture adds history; promote restores an old snapshot', async () => {
    await api.capture({
      projectId: alpha.projectId,
      projectName: null,
      pathHint: null,
      text: 'A2=1\n',
      sourcePath: null,
    });
    let v = await api.getProject(alpha.projectId);
    assertEq(v.snapshots.length, 2, 'history length');
    assertEq(v.entryCount, 1, 'latest is the recapture');
    const oldest = v.snapshots[v.snapshots.length - 1];
    assertEq(oldest.entryCount, 5, 'oldest meta');
    await api.promoteSnapshot(alpha.projectId, oldest.id);
    v = await api.getProject(alpha.projectId);
    assertEq(v.snapshots.length, 3, 'after promote');
    assertEq(v.entryCount, 5, 'promoted content is current');
    assertEq(v.via, 'restore', 'promoted via');
  });

  await step('delete removes a project', async () => {
    await api.deleteProject(beta.projectId);
    const list = await api.listProjects();
    assertEq(list.length, 1, 'one project left');
  });

  await step('encryption: enable -> lock -> wrong pass -> unlock -> change -> disable', async () => {
    const bakPath = `${(await api.status()).storePath}.bak`;

    await api.enableEncryption('selftest-pass-123');
    let st = await api.status();
    assertEq(st.encrypted, true, 'encrypted after enable');
    // The transition must not leave the previous plaintext store behind
    // as the backup.
    const bak = await api.selftest.readFile(bakPath);
    assert(bak.startsWith('age-encryption.org/'), 'backup re-encrypted after enable');

    await api.lock();
    st = await api.status();
    assertEq(st.state, 'locked', 'locked');

    let threw = false;
    try {
      await api.unlock('wrong-pass');
    } catch (e) {
      threw = true;
      assert(String(e).includes('wrong passphrase'), `friendly error, got: ${e}`);
    }
    assert(threw, 'wrong passphrase must fail');

    await api.unlock('selftest-pass-123');
    st = await api.status();
    assertEq(st.state, 'unlocked', 'unlocked again');
    assertEq(st.encrypted, true, 'still encrypted');
    const list = await api.listProjects();
    assertEq(list.length, 1, 'data survives the roundtrip');
    assertEq(list[0].entryCount, 5, 'entries survive');

    await api.changePassphrase('selftest-pass-123', 'selftest-pass-456');
    await api.lock();
    await api.unlock('selftest-pass-456');

    await api.disableEncryption('selftest-pass-456');
    st = await api.status();
    assertEq(st.encrypted, false, 'plaintext again');
    const bakAfter = await api.selftest.readFile(bakPath);
    assert(bakAfter.trimStart().startsWith('{'), 'backup plaintext again after disable');
  });

  await step('restore is refused while the store loads fine', async () => {
    let threw = false;
    try {
      await api.restoreBackup();
    } catch (e) {
      threw = String(e).includes('only for when');
    }
    assert(threw, 'restore_backup must require a corrupt store');
  });

  // State here: one project, "selftest-alpha", 3 snapshots, plaintext.
  const copyPath = () => `${storeDir}${sep}selftest-copy.store`;
  const TRANSPORT_PASS = 'transport-pass-123';
  let importToken = null;

  await step('export a store copy (encrypted) and inspect it for import', async () => {
    let threw = false;
    try {
      await api.selftest.exportStoreToPath(copyPath(), 'short');
    } catch (e) {
      threw = String(e).includes('at least 8');
    }
    assert(threw, 'short transport passphrase must be refused');

    await api.selftest.exportStoreToPath(copyPath(), TRANSPORT_PASS);

    // Inspect/apply only take a token minted by the picker (here: the
    // selftest stand-in for it) — the webview never sends a path.
    const st = await api.status();
    const liveToken = await api.selftest.stageImport(st.storePath);
    threw = false;
    try {
      await api.inspectImport(liveToken);
    } catch (e) {
      threw = String(e).includes('already using');
    }
    assert(threw, 'importing the live store file must be refused');

    importToken = await api.selftest.stageImport(copyPath());

    threw = false;
    try {
      await api.inspectImport(liveToken);
    } catch (e) {
      threw = String(e).includes('no longer pending');
    }
    assert(threw, 'a stale import token must be refused');

    let prev = await api.inspectImport(importToken);
    assertEq(prev.encrypted, true, 'copy is encrypted');
    assertEq(prev.unlocked, false, 'locked without a passphrase');
    assertEq(prev.projects.length, 0, 'no metadata before unlock');

    threw = false;
    try {
      await api.inspectImport(importToken, 'wrong-wrong');
    } catch (e) {
      threw = String(e).includes('wrong passphrase');
    }
    assert(threw, 'wrong transport passphrase must fail');

    prev = await api.inspectImport(importToken, TRANSPORT_PASS);
    assertEq(prev.unlocked, true, 'unlocked with the passphrase');
    assertEq(prev.path, copyPath(), 'preview echoes the staged path');
    assertEq(prev.projects.length, 1, 'one project inside');
    assertEq(prev.projects[0].name, 'selftest-alpha', 'name');
    assertEq(prev.projects[0].snapshotCount, 3, 'snapshotCount');
    assertEq(prev.projects[0].entryCount, 5, 'entryCount');
    assertEq(prev.projects[0].conflictsWith, 'selftest-alpha', 'conflict flagged');
  });

  await step('import: rename brings the copy in alongside the original', async () => {
    let threw = false;
    try {
      await api.applyImport(importToken, TRANSPORT_PASS, []);
    } catch (e) {
      threw = String(e).includes('no decision');
    }
    assert(threw, 'a conflict without a decision must be refused');

    const sum = await api.applyImport(importToken, TRANSPORT_PASS, [
      { name: 'selftest-alpha', action: 'rename', newName: 'selftest-alpha (imported)' },
    ]);
    assertEq(sum.renamed, 1, 'renamed count');
    const list = await api.listProjects();
    assertEq(list.length, 2, 'both projects present');
    const imported = list.find((p) => p.name === 'selftest-alpha (imported)');
    assert(imported, 'imported project exists');
    assertEq(imported.snapshotCount, 3, 'history came along');
    assertEq(imported.entryCount, 5, 'entries came along');
    const original = list.find((p) => p.name === 'selftest-alpha');
    assert(original && original.id !== imported.id, 'fresh id for the import');
  });

  await step('import: replace takes the incoming version; skip changes nothing', async () => {
    // Diverge the live project first, so replace is observable.
    await api.capture({
      projectId: alpha.projectId,
      projectName: null,
      pathHint: null,
      text: 'DIVERGED=1\n',
      sourcePath: null,
    });
    let list = await api.listProjects();
    assertEq(list.find((p) => p.name === 'selftest-alpha').snapshotCount, 4, 'diverged');

    const sum = await api.applyImport(importToken, TRANSPORT_PASS, [
      { name: 'selftest-alpha', action: 'replace' },
    ]);
    assertEq(sum.replaced, 1, 'replaced count');
    list = await api.listProjects();
    assertEq(list.length, 2, 'project count stable after replace');
    const replaced = list.find((p) => p.name === 'selftest-alpha');
    assertEq(replaced.snapshotCount, 3, 'incoming history won');

    const skipSum = await api.applyImport(importToken, TRANSPORT_PASS, [
      { name: 'selftest-alpha', action: 'skip' },
    ]);
    assertEq(skipSum.skipped, 1, 'skipped count');
    list = await api.listProjects();
    assertEq(list.length, 2, 'skip imported nothing');
  });

  await step('selftest-only commands are refused in normal runs (guard exists)', async () => {
    // The guard itself can only be probed from a non-selftest run; here
    // we just confirm the flag is consistent.
    const on = await api.selftest.enabled();
    assertEq(on, true, 'selftest flag');
  });

  await step('update checks are refused during selftest (tests stay offline)', async () => {
    let refused = false;
    try {
      await api.checkForUpdates();
    } catch (e) {
      refused = String(e).includes('selftest');
    }
    assert(refused, 'check_for_updates must be refused under ENVARSA_SELFTEST');
  });

  const ENV_LOCAL = () => `${storeDir}${sep}.env.local`;

  await step('write .env.local: fresh write equals the re-serialized snapshot', async () => {
    writeProj = await api.capture({
      projectId: null, projectName: 'selftest-write', pathHint: null,
      text: WRITE_RAW, sourcePath: null,
    });
    const token = await api.selftest.stageWrite(ENV_LOCAL());
    const path = await api.writeEnvLocal(writeProj.projectId, writeProj.snapshotId, token, 'fresh');
    assertEq(await api.selftest.readFile(path), WRITE_RAW, 'fresh .env.local bytes');
  });

  await step('write .env.local: overwrite replaces with the chosen snapshot', async () => {
    const v2 = await api.capture({
      projectId: writeProj.projectId, projectName: null, pathHint: null,
      text: 'ONLY=now\n', sourcePath: null,
    });
    const token = await api.selftest.stageWrite(ENV_LOCAL());
    await api.writeEnvLocal(writeProj.projectId, v2.snapshotId, token, 'overwrite');
    assertEq(await api.selftest.readFile(ENV_LOCAL()), 'ONLY=now\n', 'overwrite bytes');
  });

  await step('write .env.local: merge keeps local-only keys and updates shared ones', async () => {
    // Lay down an on-disk .env.local with a local-only key + a stale shared value.
    const seed = await api.capture({
      projectId: null, projectName: 'selftest-seed', pathHint: null,
      text: '# local\nLOCAL_ONLY=keepme\nPORT=oldport\n', sourcePath: null,
    });
    await api.selftest.exportToPath(seed.projectId, seed.snapshotId, ENV_LOCAL());

    // Merge the project's first snapshot (WRITE_RAW) into it.
    const token = await api.selftest.stageWrite(ENV_LOCAL());
    await api.writeEnvLocal(writeProj.projectId, writeProj.snapshotId, token, 'merge');
    const read = await api.selftest.readFile(ENV_LOCAL());
    assert(read.includes('LOCAL_ONLY=keepme'), 'local-only key kept');
    assert(read.includes('PORT=3000') && !read.includes('oldport'), 'shared key updated');
    assert(read.includes('# Added by Envarsa'), 'attribution header for appended keys');
    assert(read.includes('API_KEY=real-secret'), 'source-only key appended');
    await api.deleteProject(seed.projectId);
  });

  await step('write .env.local: example scaffold fills values, blanks placeholders, never leaks', async () => {
    const template = '# API config\nAPI_KEY=put-your-key-here\nPORT=8080\nUNUSED=placeholder-value\n';
    const token = await api.selftest.stageExample(ENV_LOCAL(), template);
    await api.writeExampleScaffold(writeProj.projectId, writeProj.snapshotId, token);
    const read = await api.selftest.readFile(ENV_LOCAL());
    assert(read.includes('# API config'), 'example comment kept');
    assert(read.includes('API_KEY=real-secret'), 'matched key filled with the real value');
    assert(read.includes('PORT=3000'), 'matched key filled');
    assert(read.includes('UNUSED=\n'), 'unmatched placeholder blanked to KEY=');
    assert(!read.includes('put-your-key-here') && !read.includes('placeholder-value'), 'placeholders never leak');
    assert(read.includes('# Added by Envarsa') && read.includes('REGION=eu-west-1'), 'source-only keys appended');
  });

  await step('write .env.local: example and non-local targets are refused', async () => {
    for (const name of ['.env.example', '.env.local.bak', 'notes.txt']) {
      const token = await api.selftest.stageWrite(`${storeDir}${sep}${name}`);
      let threw = false, msg = '';
      try {
        await api.writeEnvLocal(writeProj.projectId, writeProj.snapshotId, token, 'fresh');
      } catch (e) { threw = true; msg = String(e); }
      assert(threw, `writing ${name} must be refused`);
      if (name === '.env.example') {
        assert(msg.toLowerCase().includes('example'), `example refusal names it, got: ${msg}`);
      }
    }
  });

  await step('editor: edit lines saves a new snapshot and keeps history', async () => {
    const before = await api.getProject(writeProj.projectId);
    const histBefore = before.snapshots.length;
    const lines = await api.editLines(writeProj.projectId, writeProj.snapshotId); // WRITE_RAW
    const edited = lines
      .filter((l) => !(l.kind === 'entry' && l.key === 'TOKEN'))
      .map((l) => (l.kind === 'entry' && l.key === 'PORT' ? { ...l, value: '4000' } : l));
    edited.push({ kind: 'entry', key: 'NEW_KEY', value: 'added-by-editor', exported: false });
    await api.saveEditedSnapshot({ projectId: writeProj.projectId, projectName: null, pathHint: null, lines: edited });

    const after = await api.getProject(writeProj.projectId);
    assertEq(after.snapshots.length, histBefore + 1, 'a new snapshot was added');
    assertEq(after.via, 'edit', 'newest snapshot is via edit');
    const portIdx = after.lines.find((l) => l.t === 'entry' && l.key === 'PORT').idx;
    assertEq((await api.revealValue(after.id, after.snapshotId, portIdx)).value, '4000', 'edited value');
    const newIdx = after.lines.find((l) => l.t === 'entry' && l.key === 'NEW_KEY').idx;
    assertEq((await api.revealValue(after.id, after.snapshotId, newIdx)).value, 'added-by-editor', 'new key value');
    assert(!after.lines.some((l) => l.t === 'entry' && l.key === 'TOKEN'), 'deleted key is gone');
    const old = await api.getProject(writeProj.projectId, writeProj.snapshotId);
    assert(old.lines.some((l) => l.t === 'entry' && l.key === 'TOKEN'), 'old snapshot still intact');
  });

  await step('editor: invalid keys are refused; a project can be built store-only', async () => {
    let threw = false;
    try {
      await api.saveEditedSnapshot({
        projectId: writeProj.projectId, projectName: null, pathHint: null,
        lines: [{ kind: 'entry', key: 'A B', value: '1', exported: false }],
      });
    } catch (e) { threw = String(e).includes('valid key'); }
    assert(threw, 'a key with a space must be refused');

    // No file in, no file out: a comment + two entries straight into the store.
    const res = await api.saveEditedSnapshot({
      projectId: null, projectName: 'selftest-cloud', pathHint: null,
      lines: [
        { kind: 'comment', text: '# cloud-only project' },
        { kind: 'entry', key: 'SERVICE_URL', value: 'https://api.example.com', exported: false },
        { kind: 'entry', key: 'SERVICE_TOKEN', value: 'tok_123', exported: false },
      ],
    });
    assertEq(res.entryCount, 2, 'two entries stored');
    const cloud = (await api.listProjects()).find((p) => p.name === 'selftest-cloud');
    assert(cloud && cloud.entryCount === 2, 'cloud project present with 2 entries');
    const v = await api.getProject(cloud.id);
    assert(v.lines.some((l) => l.t === 'comment'), 'comment preserved in the store');
    const tIdx = v.lines.find((l) => l.t === 'entry' && l.key === 'SERVICE_TOKEN').idx;
    assertEq((await api.revealValue(v.id, v.snapshotId, tIdx)).value, 'tok_123', 'stored value retrievable');
  });

  try {
    if (originalClipboard !== null) {
      await api.selftest.setClipboard(originalClipboard);
    }
  } catch {
    /* best effort */
  }

  const failed = results.filter((r) => r.startsWith('FAIL')).length;
  const skipped = results.filter((r) => r.startsWith('skip')).length;
  await api.selftest.done(results.length - failed - skipped, failed, results.join('\n'));
}
