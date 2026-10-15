#!/usr/bin/env python3
"""End-to-end test of ibus-engine-pkey against a real, private ibus-daemon.

Run through scripts/linux/e2e-ibus.sh, which starts the daemon and sets IBUS_ADDRESS. The test
plays the part of an application: it creates an input context, selects the pKey engine and sends
key events exactly as the toolkit would, then reads the commit and preedit signals back.
"""
import os
import sys
import time

import gi

gi.require_version("IBus", "1.0")
from gi.repository import GLib, IBus  # noqa: E402

SHIFT, CTRL = 1, 4
RELEASE = 1 << 30
KEY_SHIFT_L, KEY_CTRL_L = 0xFFE1, 0xFFE3
KEY_BACKSPACE, KEY_RETURN = 0xFF08, 0xFF0D


class App:
    """A fake application: a text buffer plus the current preedit."""

    def __init__(self, bus, client="pkey-e2e"):
        self.bus = bus
        self.ic = bus.create_input_context(client)
        self.ic.set_capabilities(
            IBus.Capabilite.PREEDIT_TEXT | IBus.Capabilite.FOCUS | IBus.Capabilite.SURROUNDING_TEXT
        )
        self.text = ""
        self.preedit = ""
        self.mode_labels = []
        self.ic.connect("delete-surrounding-text", self._delete)
        self.ic.connect("commit-text", self._commit)
        self.ic.connect("update-preedit-text", self._preedit)
        self.ic.connect("hide-preedit-text", self._hide)
        self.ic.connect("update-property", self._prop)
        self.ic.connect("register-properties", self._props)
        self.pump()

    def _delete(self, _ic, offset, n_chars):
        # Only "n characters right before the cursor" is used by the engine.
        assert offset == -n_chars, (offset, n_chars)
        self.text = self.text[: len(self.text) - n_chars]

    def _commit(self, _ic, text):
        self.text += text.get_text()

    def _preedit(self, _ic, text, _cursor, visible, _mode=0):
        self.preedit = text.get_text() if visible else ""

    def _hide(self, _ic):
        self.preedit = ""

    def _prop(self, _ic, prop):
        self.mode_labels.append(prop.get_label().get_text())

    def _props(self, _ic, plist):
        self.mode_labels.append(plist.get(0).get_label().get_text())

    def pump(self, seconds=0.15):
        ctx = GLib.MainContext.default()
        end = time.time() + seconds
        while time.time() < end:
            while ctx.iteration(False):
                pass
            time.sleep(0.005)

    def key(self, keyval, state=0, keycode=0):
        """Press and release; a key the engine does not eat reaches the 'application'."""
        handled = self.ic.process_key_event(keyval, keycode, state)
        self.ic.process_key_event(keyval, keycode, state | RELEASE)
        self.pump(0.02)
        if not handled:
            if keyval == 0x20:
                self.text += " "
            elif keyval == KEY_BACKSPACE:
                self.text = self.text[:-1]
        return handled

    def type(self, s):
        for ch in s:
            self.key(ord(ch), SHIFT if ch.isupper() else 0)

    def modifier_tap(self, *keyvals):
        for k in keyvals:
            self.ic.process_key_event(k, 0, 0)
        for k in reversed(keyvals):
            self.ic.process_key_event(k, 0, RELEASE)
        self.pump(0.05)


def check(name, got, want):
    ok = got == want
    print(("PASS " if ok else "FAIL ") + name + ("" if ok else f"\n   got  {got!r}\n   want {want!r}"))
    return ok


def main():
    bus = IBus.Bus()
    if not bus.is_connected():
        print("cannot connect to the private ibus-daemon")
        return 2
    results = []
    # What GNOME does when the user picks pKey as the input source.
    results.append(check("select pkey as global engine", bus.set_global_engine("pkey"), True))
    app = App(bus)
    app.ic.focus_in()
    app.pump(0.8)

    engines = [e.get_name() for e in bus.list_engines()]
    results.append(check("engine pkey is registered", "pkey" in engines, True))

    app.type("tieengs")
    results.append(check("telex preedit", app.preedit, "tiếng"))
    app.key(0x20)
    results.append(check("space commits the word", app.text, "tiếng "))
    results.append(check("preedit cleared after commit", app.preedit, ""))

    app.text = ""
    app.type("Vieejt Nam ")
    results.append(check("sentence", app.text, "Việt Nam "))

    app.text = ""
    app.type("hoaf")
    app.key(KEY_BACKSPACE)
    results.append(check("backspace edits the preedit", app.preedit, "hò"))
    app.key(KEY_RETURN)
    results.append(check("enter commits", app.text, "hò"))

    app.text = ""
    app.type("a")
    app.key(ord("c"), CTRL)
    results.append(check("ctrl+c commits then passes", (app.text, app.preedit), ("a", "")))

    # Ctrl+Shift toggles Vietnamese/English; English passes letters straight through.
    app.text = ""
    app.modifier_tap(KEY_CTRL_L, KEY_SHIFT_L)
    handled = app.key(ord("a"))
    results.append(check("english mode passes keys", handled, False))
    app.modifier_tap(KEY_CTRL_L, KEY_SHIFT_L)
    app.type("as")
    results.append(check("back to vietnamese", app.preedit, "á"))
    app.key(0x20)

    # A password field is never composed.
    pw = App(bus, "pkey-e2e-password")
    pw.ic.set_content_type(IBus.InputPurpose.PASSWORD, 0)
    pw.ic.focus_in()
    pw.pump(0.5)
    handled = pw.key(ord("a"))
    results.append(check("password field passes keys", (handled, pw.preedit), (False, "")))

    # A second input context does not share the word in progress.
    one, two = App(bus, "pkey-e2e-one"), App(bus, "pkey-e2e-two")
    one.ic.focus_in()
    one.pump(0.5)
    one.type("vie")
    one.ic.focus_out()
    one.pump(0.2)
    two.ic.focus_in()
    two.pump(0.3)
    two.type("a")
    results.append(check("contexts are independent", two.preedit, "a"))
    # The daemon commits the preedit of the context that lost focus (preedit mode COMMIT).
    results.append(check("focus-out keeps the word", one.text + one.preedit, "vie"))
    one.ic.focus_in()
    one.pump(0.3)
    one.type("e")
    results.append(check("a new word starts after refocus", one.preedit, "e"))

    # Inline mode: no preedit at all (so nothing is drawn underlined); the word is typed into the
    # text field and corrected in place with DeleteSurroundingText.
    cfg_dir = os.environ["PKEY_CONFIG_DIR"]
    os.makedirs(cfg_dir, exist_ok=True)
    with open(os.path.join(cfg_dir, "config.toml"), "w") as f:
        f.write("schema_version = 1\n[compat]\nlinux_preedit = false\n")
    inl = App(bus, "pkey-e2e-inline")
    inl.ic.focus_in()
    inl.pump(0.6)
    inl.type("tieengs")
    results.append(check("inline: word typed in place, no preedit", (inl.text, inl.preedit), ("tiếng", "")))
    inl.key(0x20)
    inl.type("Vieejt")
    results.append(check("inline: second word", inl.text, "tiếng Việt"))
    inl.key(KEY_BACKSPACE)
    inl.key(KEY_BACKSPACE)
    results.append(check("inline: backspace edits in place", inl.text, "tiếng Vị"))

    failed = results.count(False)
    print(f"\n{len(results) - failed}/{len(results)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
