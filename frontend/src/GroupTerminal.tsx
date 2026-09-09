import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  onPtyOutput,
  startInteractiveSession,
  sendInteractiveText,
  interactiveScreen,
  resizeInteractive,
} from "./api";

/** A live embedded terminal for one group's `claude` PTY. Renders the raw PTY
 *  output faithfully (colors, cursor, spinners) via xterm.js and forwards direct
 *  keystrokes back to the session, so the card behaves like the real terminal
 *  `claude` runs in — while the global prompt bar can also drive it (@mention).
 *
 *  Lifecycle: on mount we subscribe to `pty://output` FIRST, then start (or
 *  reuse) the session, then seed the current rendered frame so a reused/idle
 *  session isn't blank. On unmount we only detach the UI — the `claude` process
 *  keeps running (persistence-free by design; the group owns its lifetime). */
export function GroupTerminal({
  sessionRef,
  cwd,
}: {
  sessionRef: string;
  /** Stored working folder — fallback used only when the session has no
   *  transcript to resume, so a never-run session still opens (starts fresh). */
  cwd?: string | null;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;

    const term = new Terminal({
      fontSize: 12,
      fontFamily:
        'Menlo, Consolas, "DejaVu Sans Mono", "Courier New", monospace',
      cursorBlink: true,
      convertEol: false,
      scrollback: 4000,
      theme: { background: "#0b0e14", foreground: "#c8d0da" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);

    // Push the fitted size to the PTY so claude's TUI repaints to this width.
    const syncSize = () => {
      try {
        fit.fit();
      } catch {
        /* container not laid out yet */
      }
      if (!disposed && term.cols > 0 && term.rows > 0) {
        void resizeInteractive(sessionRef, term.rows, term.cols);
      }
    };

    // User typing in the terminal goes straight to the session (real terminal).
    const dataSub = term.onData((data) => {
      void sendInteractiveText(sessionRef, data);
    });

    (async () => {
      // 1) Subscribe before starting so the opening banner isn't missed.
      unlisten = await onPtyOutput((chunk) => {
        if (disposed || chunk.id !== sessionRef) return;
        term.write(new Uint8Array(chunk.bytes));
      });
      if (disposed) return;

      // 2) Start (or reuse) the session's PTY (cwd is the transcript-less
      //    fallback so a never-run session still opens instead of erroring).
      try {
        await startInteractiveSession(sessionRef, undefined, cwd);
      } catch (e) {
        // Backend rejects with a `CommandError` = `{ message }` object, so a bare
        // String(e) rendered as the useless "[object Object]". Pull the message.
        const msg =
          e && typeof e === "object" && "message" in e
            ? String((e as { message: unknown }).message)
            : String(e);
        term.write(`\r\n\x1b[31m세션 시작 실패: ${msg}\x1b[0m\r\n`);
        return;
      }
      if (disposed) return;

      // 3) Fit + tell the PTY our size, then seed the current frame so a reused
      //    or idle session shows its last state instead of a blank screen.
      syncSize();
      try {
        const scr = await interactiveScreen(sessionRef);
        if (!disposed && scr.lines.length) {
          term.write("\x1b[2J\x1b[H" + scr.lines.join("\r\n"));
        }
      } catch {
        /* seeding is best-effort */
      }
    })();

    const ro = new ResizeObserver(() => syncSize());
    ro.observe(host);

    return () => {
      disposed = true;
      ro.disconnect();
      dataSub.dispose();
      if (unlisten) unlisten();
      term.dispose();
    };
  }, [sessionRef, cwd]);

  return <div className="group-term" ref={hostRef} />;
}
