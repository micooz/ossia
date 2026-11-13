window.__ModuleLoader__.load({
  id: 'ossia-dsh-plugin',
  factory(require) {
    const React = require('react');
    const CHANNEL = '/ossia-selection';
    const POLL_MS = 500;
    const SEPARATOR = '\n\n';
    const MESSAGES = {
      en: {
        hintOne: '/ossia 1 selection selected.',
        hintMany: '/ossia {count} selections selected.',
      },
      zh: {
        hintOne: '/ossia 已选择 {count} 个对象。',
        hintMany: '/ossia 已选择 {count} 个对象。',
      },
      ja: {
        hintOne: '/ossia {count} 件の選択範囲。',
        hintMany: '/ossia {count} 件の選択範囲。',
      },
      es: {
        hintOne: '/ossia {count} selección.',
        hintMany: '/ossia {count} selecciones.',
      },
      fr: {
        hintOne: '/ossia {count} sélection.',
        hintMany: '/ossia {count} sélections.',
      },
      de: {
        hintOne: '/ossia {count} Auswahl.',
        hintMany: '/ossia {count} Auswahlen.',
      },
      ru: {
        hintOne: '/ossia Выбран {count} фрагмент.',
        hintMany: '/ossia Выбрано фрагментов: {count}.',
      },
      pt: {
        hintOne: '/ossia {count} seleção.',
        hintMany: '/ossia {count} seleções.',
      },
    };
    const LEGACY_HINTS = ['/ossia 已选择 {count} 个选区。'];
    const HINT = new RegExp(
      `^(?:${Object.values(MESSAGES)
        .flatMap((messages) => [messages.hintOne, messages.hintMany])
        .concat(LEGACY_HINTS)
        .map(templatePattern)
        .join('|')})`,
    );
    const applied = { epoch: null, revision: -1 };
    let warned = false;

    function templatePattern(template) {
      return template
        .split('{count}')
        .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
        .join('\\d+');
    }

    function format(template, params) {
      return template.replace(/\{(\w+)\}/g, (match, name) =>
        name in params ? String(params[name]) : match,
      );
    }

    function translate(locale, key, params = {}) {
      const messages = MESSAGES[locale] ?? MESSAGES.en;
      return format(messages[key] ?? MESSAGES.en[key] ?? key, params);
    }

    function englishT(key, params = {}) {
      return translate('en', key, params);
    }

    function hintFor(count, t = englishT) {
      if (count <= 0) return null;
      return t(count === 1 ? 'hintOne' : 'hintMany', { count });
    }

    function reconcile(draft, count, t = englishT) {
      const match = HINT.exec(draft);
      const hint = hintFor(count, t);
      if (hint === null) {
        if (match === null) return null;
        let end = match[0].length;
        while (draft[end] === '\n' || draft[end] === '\r') end += 1;
        return { start: 0, end, text: '' };
      }
      if (match !== null) {
        if (match[0] === hint) return null;
        return { start: 0, end: match[0].length, text: hint };
      }
      if (draft.length === 0) return { start: 0, end: 0, text: hint };
      return { start: 0, end: 0, text: `${hint}${SEPARATOR}` };
    }

    function warnMissing() {
      if (warned) return;
      warned = true;
      console.warn(
        '[ossia-dsh-plugin] composer inputActions.insertText is unavailable; selection hints are disabled',
      );
    }

    function OssiaSelectionHintHost(props) {
      const revision = React.useSyncExternalStore(
        (listener) => props.locale.subscribe(listener),
        () => props.locale.getSnapshot().revision,
      );
      return React.createElement(OssiaSelectionHint, { ...props, localeRevision: revision });
    }

    function OssiaSelectionHint(props) {
      const draft = props.useInput((state) => state.draft);
      const draftRev = props.useInput((state) => state.draftRev);
      const live = React.useRef({ draft, draftRev });
      live.current = { draft, draftRev };
      const mountedAt = React.useRef(Date.now());
      const lastCount = React.useRef(null);

      React.useEffect(() => {
        if (
          props.inputActions === undefined ||
          typeof props.inputActions.insertText !== 'function'
        ) {
          warnMissing();
          return;
        }
        mountedAt.current = Date.now();
        let stopped = false;
        let busy = false;
        const tick = async () => {
          if (stopped || busy) return;
          busy = true;
          try {
            const result = await props.rpc.call(CHANNEL, 'latest', {});
            if (!result?.ok || !result.value) return;
            const latest = result.value;
            if (!Number.isInteger(latest.count) || latest.count < 0) return;
            const seen = applied.epoch === latest.epoch ? applied.revision : -1;
            if (latest.revision <= seen) return;
            const current = live.current;
            const change = reconcile(current.draft, latest.count, props.t);
            const recent = latest.at >= mountedAt.current - 1000;
            if (!recent && applied.epoch !== latest.epoch) {
              applied.epoch = latest.epoch;
              applied.revision = latest.revision;
              return;
            }
            lastCount.current = latest.count;
            if (change === null) {
              applied.epoch = latest.epoch;
              applied.revision = latest.revision;
              return;
            }
            if (
              props.inputActions.insertText(change.text, {
                start: change.start,
                end: change.end,
                draftRev: current.draftRev,
              })
            ) {
              applied.epoch = latest.epoch;
              applied.revision = latest.revision;
            }
          } catch {
            // DSH connection may not be ready yet; retry on the next tick.
          } finally {
            busy = false;
          }
        };
        void tick();
        const timer = setInterval(tick, POLL_MS);
        return () => {
          stopped = true;
          clearInterval(timer);
        };
      }, [props.inputActions, props.rpc, props.session?.sessionId, props.t]);

      React.useEffect(() => {
        if (lastCount.current === null) return;
        if (lastCount.current > 0 && !HINT.test(live.current.draft)) return;
        const change = reconcile(live.current.draft, lastCount.current, props.t);
        if (change === null || props.inputActions === undefined) return;
        props.inputActions.insertText(change.text, {
          start: change.start,
          end: change.end,
          draftRev: live.current.draftRev,
        });
      }, [props.localeRevision, props.inputActions, props.t]);

      return null;
    }

    function apply(ctx) {
      const rpc = ctx.connection.rpc;
      const t = ctx.locale.bind('ossia');
      ctx.effect(() => ctx.locale.register('ossia', MESSAGES), 'ossia-selection: locale');
      ctx.slots.inject('conversation.input.dock', () =>
        ctx.slots.register(
          { name: 'conversation.input.dock', id: 'ossia-selection-hint', order: 1000 },
          (props) =>
            React.createElement(OssiaSelectionHintHost, { ...props, rpc, t, locale: ctx.locale }),
        ),
      );
    }

    return {
      inject: ['slots', 'connection', 'locale'],
      apply,
      __test: { hintFor, reconcile, translate },
    };
  },
});
