"""
Cross-platform encrypted key-value store backed by SQLite + Fernet.
"""
import sqlite3

from cryptography.fernet import Fernet

from src.core.paths import CONFIG_DIR, log

DB_PATH = CONFIG_DIR / "secure_settings.db"
KEY_PATH = CONFIG_DIR / ".keyfile"


def _get_fernet():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    if KEY_PATH.exists():
        key = KEY_PATH.read_bytes()
    else:
        key = Fernet.generate_key()
        KEY_PATH.write_bytes(key)
    return Fernet(key)


def _store_secret(key: str, value: str):
    try:
        f = _get_fernet()
        encrypted = f.encrypt(value.encode("utf-8"))
        conn = sqlite3.connect(str(DB_PATH))
        conn.execute("CREATE TABLE IF NOT EXISTS secrets (key TEXT PRIMARY KEY, value BLOB)")
        conn.execute("INSERT OR REPLACE INTO secrets (key, value) VALUES (?, ?)", (key, encrypted))
        conn.commit()
        conn.close()
    except Exception as e:
        log(f"Failed to store secret {key}: {e}")

def _read_secret(key: str) -> str | None:
    try:
        if not DB_PATH.exists():
            return None
        f = _get_fernet()
        conn = sqlite3.connect(str(DB_PATH))
        cur = conn.execute("SELECT value FROM secrets WHERE key = ?", (key,))
        row = cur.fetchone()
        conn.close()
        if row:
            return f.decrypt(row[0]).decode("utf-8")
        return None
    except Exception as e:
        log(f"Failed to read secret {key}: {e}")
        return None

def _delete_secret(key: str):
    try:
        if DB_PATH.exists():
            conn = sqlite3.connect(str(DB_PATH))
            conn.execute("DELETE FROM secrets WHERE key = ?", (key,))
            conn.commit()
            conn.close()
    except Exception:
        pass

def store_api_key(api_key: str):
    _store_secret("api_key", api_key)

def read_api_key() -> str | None:
    return _read_secret("api_key")

# --- Multi-API helpers (v2) ---
def store_api_key_for_id(api_id: str, api_key: str):
    _store_secret(f"api:{api_id}:key", api_key)

def read_api_key_for_id(api_id: str) -> str | None:
    # try per-id first, fallback to legacy global for migration
    v = _read_secret(f"api:{api_id}:key")
    if v is not None:
        return v
    # fallback: if this is the first migrated api, try global key
    return _read_secret("api_key") if api_id else None

def delete_api_credentials(api_id: str):
    _delete_secret(f"api:{api_id}:key")
    # legacy opencode_go workspace/cookie entries (pre-unified scheme)
    _delete_secret(f"opencode_go:{api_id}:workspace_id")
    _delete_secret(f"opencode_go:{api_id}:auth_cookie")
