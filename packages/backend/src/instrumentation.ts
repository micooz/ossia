// Startup self-heal (ADR-0002 §8): verify the vendored engine once, at boot, instead of letting the
// first render fail inside a Guile traceback. `register()` runs before any request is served, so the
// log line is the first thing an operator sees.
//
// The repair itself lives in engine.ts (`healMtimes`) and is applied lazily by `engine()`; here we
// only resolve + report, because a thrown error at boot would take the whole API down for a problem
// that only affects rendering.
export async function register(): Promise<void> {
  if (process.env.NEXT_RUNTIME !== 'nodejs') return;
  const { engine, candidateRoots, verifyTree, readLock } = await import('./server/engine.ts');
  try {
    const info = engine();
    console.log(`[ossia] engine ok [${info.src}] lilypond ${info.version}`);
  } catch (e) {
    // Report every candidate we looked at, with the home directory redacted (G6).
    const home = process.env.HOME ?? '';
    const tried = candidateRoots()
      .map((c) => {
        const problems = verifyTree(c.dir, readLock());
        return `${c.src}: ${problems.length ? problems.join('; ') : 'absent/unusable'}`;
      })
      .join(' | ')
      .replace(home, '~');
    console.error(
      `[ossia] engine unusable: ${(e as Error).message.split('\n').join(' ')} (looked in ${tried})`,
    );
    console.error(
      '[ossia] fix: pnpm engine:install  (renders retry the mtime self-heal on demand)',
    );
  }
}
