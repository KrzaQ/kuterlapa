#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pynacl>=1.5"]
# ///
"""KeePassXC for Kuterlapa's model tools, through KeePassXC's browser
integration protocol (the one its browser extension uses). KeePassXC must
run with an unlocked database and Settings → Browser Integration on.

    keepass.py connect                  connect once; KeePassXC asks the user
                                        to allow it and to name it
    keepass.py find <url>               entries for an address: title, user
                                        name, group; never a password
    keepass.py groups                   the database's groups
    keepass.py add <url> <user> [group] a new entry with a new random password
    keepass.py type <url> [user] [enter] type an entry's password into the
                                        window in front, after the user says
                                        yes in Kuterlapa's overlay; "enter"
                                        presses Return after it

The address can be any scheme: https://shop.example, rdp://10.0.0.5,
ssh://host. KeePassXC matches entries by their URL's host.

Passwords never reach the model: "add" makes one and hands it straight to
KeePassXC, "find" leaves them out, and "type" sends it only to Kuterlapa's
app, which types it. The connection's key is kept in the
macOS keychain (service "kuterlapa-keepassxc").
"""

import base64
import json
import os
import secrets
import string
import socket
import struct
import subprocess
import sys
from urllib.parse import urlparse

from nacl.public import Box, PrivateKey, PublicKey

PROXY = "/Applications/KeePassXC.app/Contents/MacOS/keepassxc-proxy"
STATE = os.path.join(os.environ.get("XDG_STATE_HOME") or os.path.expanduser("~/.local/state"), "kuterlapa")
KEYCHAIN = "kuterlapa-keepassxc"


def b64(data: bytes) -> str:
    return base64.b64encode(data).decode()


def unb64(text: str) -> bytes:
    return base64.b64decode(text)


def incremented(nonce: bytes) -> bytes:
    """libsodium's sodium_increment: the nonce as a little-endian number + 1."""
    value = int.from_bytes(nonce, "little") + 1
    return (value % (1 << (8 * len(nonce)))).to_bytes(len(nonce), "little")


class Fail(Exception):
    pass


class KeePassXC:
    """One session with KeePassXC through keepassxc-proxy (native messaging:
    a 4-byte length, then JSON)."""

    def __init__(self):
        if not os.path.exists(PROXY):
            raise Fail("KeePassXC is not installed in /Applications.")
        self.proxy = subprocess.Popen([PROXY], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.client_id = b64(secrets.token_bytes(24))
        self.key = PrivateKey.generate()
        nonce = secrets.token_bytes(24)
        answer = self.send({
            "action": "change-public-keys",
            "publicKey": b64(bytes(self.key.public_key)),
            "nonce": b64(nonce),
            "clientID": self.client_id,
        })
        if not answer.get("success") or "publicKey" not in answer:
            raise Fail("KeePassXC did not answer. Is Browser Integration on in its settings?")
        self.box = Box(self.key, PublicKey(unb64(answer["publicKey"])))

    def send(self, message: dict) -> dict:
        data = json.dumps(message).encode()
        try:
            self.proxy.stdin.write(struct.pack("<I", len(data)) + data)
            self.proxy.stdin.flush()
            head = self.proxy.stdout.read(4)
        except BrokenPipeError:
            head = b""
        if len(head) < 4:
            raise Fail("KeePassXC closed the connection. Is it running, with Browser Integration on?")
        return json.loads(self.proxy.stdout.read(struct.unpack("<I", head)[0]))

    def call(self, action: str, fields: dict) -> dict:
        """An encrypted request; returns the decrypted answer."""
        nonce = secrets.token_bytes(24)
        inner = json.dumps({"action": action, **fields}).encode()
        answer = self.send({
            "action": action,
            "message": b64(self.box.encrypt(inner, nonce).ciphertext),
            "nonce": b64(nonce),
            "clientID": self.client_id,
        })
        if "message" not in answer:
            code = str(answer.get("errorCode"))
            if code == "1":
                raise Fail("The KeePassXC database is locked. Ask the user to unlock it.")
            # "No logins found": an empty answer, not a failure.
            if code == "15":
                return {"success": True, "entries": []}
            raise Fail(f"KeePassXC refused {action}: {answer.get('error') or answer}")
        reply_nonce = unb64(answer["nonce"])
        if reply_nonce != incremented(nonce):
            raise Fail("KeePassXC's answer had a wrong nonce.")
        return json.loads(self.box.decrypt(unb64(answer["message"]), reply_nonce))

    def close(self):
        try:
            self.proxy.stdin.close()
            self.proxy.wait(timeout=2)
        except Exception:
            self.proxy.kill()


def database_hash(kp: KeePassXC) -> str:
    answer = kp.call("get-databasehash", {})
    if not answer.get("hash"):
        raise Fail("KeePassXC has no open database.")
    return answer["hash"]


def saved_identity(db_hash: str) -> dict | None:
    found = subprocess.run(
        ["/usr/bin/security", "find-generic-password", "-s", KEYCHAIN, "-a", db_hash, "-w"],
        capture_output=True, text=True,
    )
    return json.loads(found.stdout) if found.returncode == 0 else None


def save_identity(db_hash: str, identity: dict):
    subprocess.run(
        ["/usr/bin/security", "add-generic-password", "-U", "-s", KEYCHAIN, "-a", db_hash,
         "-l", "Kuterlapa: KeePassXC connection", "-w", json.dumps(identity)],
        check=True, capture_output=True,
    )


def identity_for(kp: KeePassXC) -> dict:
    """The saved connection to the open database, checked with KeePassXC."""
    identity = saved_identity(database_hash(kp))
    if not identity:
        raise Fail("Kuterlapa is not connected to this KeePassXC database yet: use keepass_connect.")
    answer = kp.call("test-associate", {"id": identity["id"], "key": identity["public"]})
    if not answer.get("success"):
        raise Fail("KeePassXC no longer knows Kuterlapa's connection: use keepass_connect again.")
    return identity


def connect(kp: KeePassXC) -> str:
    db_hash = database_hash(kp)
    old = saved_identity(db_hash)
    if old and kp.call("test-associate", {"id": old["id"], "key": old["public"]}).get("success"):
        return f"Already connected to KeePassXC as \"{old['id']}\"."
    id_key = PrivateKey.generate()
    answer = kp.call("associate", {"key": b64(bytes(kp.key.public_key)), "idKey": b64(bytes(id_key.public_key))})
    if not answer.get("success") or not answer.get("id"):
        raise Fail("The user did not allow the connection in KeePassXC.")
    # The protocol only ever sends the public part again.
    save_identity(db_hash, {"id": answer["id"], "public": b64(bytes(id_key.public_key))})
    return f"Connected to KeePassXC as \"{answer['id']}\"."


def normalized(url: str) -> str:
    """An address with a scheme; "shop.example" becomes https://shop.example."""
    url = url.strip()
    if "://" not in url:
        url = "https://" + url
    if not urlparse(url).hostname:
        raise Fail(f"Not an address: {url}")
    return url


def find(kp: KeePassXC, url: str) -> str:
    identity = identity_for(kp)
    url = normalized(url)
    answer = kp.call("get-logins", {"url": url, "submitUrl": url, "keys": [{"id": identity["id"], "key": identity["public"]}]})
    entries = answer.get("entries") or []
    if not entries:
        return f"No entries for {url}."
    lines = [f"{len(entries)} entries for {url} (passwords are not shown):"]
    for e in entries:
        group = f" | group: {e['group']}" if e.get("group") else ""
        lines.append(f"- {e.get('name') or '(no title)'} | user: {e.get('login') or '(none)'}{group}")
    return "\n".join(lines)


def group_paths(nodes: list, prefix: str = "") -> list[tuple[str, str]]:
    out = []
    for node in nodes:
        path = f"{prefix}/{node['name']}" if prefix else node["name"]
        out.append((path, node["uuid"]))
        out.extend(group_paths(node.get("children") or [], path))
    return out


def groups(kp: KeePassXC) -> list[tuple[str, str]]:
    identity_for(kp)
    answer = kp.call("get-database-groups", {})
    # {"groups": {"groups": [root]}} in KeePassXC 2.7; a plain list elsewhere.
    tree = answer.get("groups") or []
    if isinstance(tree, dict):
        tree = tree.get("groups") or []
    return group_paths(tree)


def new_password(length: int = 24) -> str:
    """Letters, digits and a few symbols that sites usually accept; at least
    one of each kind."""
    symbols = "!#%+-.=?@_"
    kinds = [string.ascii_lowercase, string.ascii_uppercase, string.digits, symbols]
    alphabet = "".join(kinds)
    while True:
        password = "".join(secrets.choice(alphabet) for _ in range(length))
        if all(any(c in kind for c in password) for kind in kinds):
            return password


def add(kp: KeePassXC, url: str, user: str, group: str | None) -> str:
    identity = identity_for(kp)
    url = normalized(url)
    existing = kp.call("get-logins", {"url": url, "submitUrl": url, "keys": [{"id": identity["id"], "key": identity["public"]}]})
    for e in existing.get("entries") or []:
        if (e.get("login") or "").lower() == user.lower():
            raise Fail(f"An entry for {user} at {url} exists already: {e.get('name')}. Nothing added.")
    group_uuid = ""
    if group:
        paths = groups(kp)
        wanted = group.strip("/").lower()
        match = [uuid for path, uuid in paths if path.lower() == wanted or path.lower().endswith("/" + wanted)]
        if len(match) != 1:
            names = ", ".join(p for p, _ in paths)
            raise Fail(f"No single group named {group}. Groups: {names}")
        group_uuid = match[0]
    answer = kp.call("set-login", {
        "url": url,
        "submitUrl": url,
        "id": identity["id"],
        "nonce": b64(secrets.token_bytes(24)),
        "login": user,
        "password": new_password(),
        "group": group or "",
        "groupUuid": group_uuid,
        "uuid": "",
        "downloadFavicon": "false",
    })
    if not answer.get("success") or answer.get("error"):
        raise Fail(f"KeePassXC did not save the entry: {answer.get('error') or answer}")
    title = urlparse(url).hostname
    return (f"Saved in KeePassXC: title {title}, user {user}, address {url}"
            f"{', group ' + group if group else ''}, with a new random password (24 characters, not shown).")


def kuterlapa(socket_name: str, line: str, wait: float = 10) -> str:
    """One request to Kuterlapa (shell.sock: the app, core.sock: the core)."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(wait)
        s.connect(os.path.join(STATE, socket_name))
        s.sendall((line.replace("\n", " ") + "\n").encode())
        data = b""
        while chunk := s.recv(4096):
            data += chunk
        return data.decode().strip()


def front_window() -> str:
    """"App — title" of the window in front."""
    parts = kuterlapa("shell.sock", "window").split("\t")
    app = parts[1] if len(parts) > 1 else "?"
    title = parts[2] if len(parts) > 2 else ""
    return f"{app} — {title}" if title else app


def type_password(kp: KeePassXC, url: str, user: str | None, enter: bool) -> str:
    identity = identity_for(kp)
    url = normalized(url)
    answer = kp.call("get-logins", {"url": url, "submitUrl": url, "keys": [{"id": identity["id"], "key": identity["public"]}]})
    entries = answer.get("entries") or []
    if user:
        entries = [e for e in entries if (e.get("login") or "").lower() == user.lower()]
    if not entries:
        raise Fail(f"No entry for {url}{' and user ' + user if user else ''}.")
    if len(entries) > 1:
        users = ", ".join(e.get("login") or "(none)" for e in entries)
        raise Fail(f"{len(entries)} entries for {url}: give the user name ({users}).")
    entry = entries[0]
    name = entry.get("name") or url
    login = entry.get("login") or "(no user name)"
    target = front_window()
    if "KeePassXC" in target or target.startswith("Kuterlapa"):
        raise Fail(f"The window in front is {target}: focus the field to fill first.")
    # The user decides, seeing where the password goes.
    question = json.dumps({
        "kind": "question", "title": "KeePass", "tool": "keepass_type", "command": "",
        "description": f"Wpisać hasło „{name}” ({login}) w okno: {target}?",
    })
    if kuterlapa("core.sock", f"approve {question}", wait=150) != "yes":
        return "The user did not allow it. Nothing was typed."
    # Something else may have come to the front while the user decided.
    if front_window() != target:
        raise Fail(f"The window in front changed to {front_window()}. Nothing was typed.")
    if kuterlapa("shell.sock", f"type {entry.get('password') or ''}") != "ok":
        raise Fail("Kuterlapa could not type.")
    if enter:
        kuterlapa("shell.sock", "key Return")
    return (f"Typed the password of {name} ({login}) into {target}"
            f"{' and pressed Return' if enter else ''}. The password was not shown.")


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in ("connect", "find", "groups", "add", "type"):
        print(__doc__.strip())
        return 2
    kp = None
    try:
        kp = KeePassXC()
        command, args = argv[0], [a for a in argv[1:] if a != ""]
        if command == "connect":
            print(connect(kp))
        elif command == "find" and args:
            print(find(kp, args[0]))
        elif command == "groups":
            print("\n".join(path for path, _ in groups(kp)) or "No groups.")
        elif command == "add" and len(args) >= 2:
            print(add(kp, args[0], args[1], args[2] if len(args) > 2 else None))
        elif command == "type" and args:
            enter = args[-1] == "enter"
            rest = args[:-1] if enter else args
            print(type_password(kp, rest[0], rest[1] if len(rest) > 1 else None, enter))
        else:
            print(__doc__.strip())
            return 2
        return 0
    except Fail as e:
        print(f"error: {e}")
        return 1
    finally:
        if kp:
            kp.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
