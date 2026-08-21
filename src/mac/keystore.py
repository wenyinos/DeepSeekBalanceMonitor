"""
Deprecated macOS keystore — unified to secure_settings (Fernet + SQLite).

v2.0: Windows and macOS now share the same encrypted storage.
This module is kept for import compatibility; encrypt/decrypt
are thin wrappers over src/secure_settings.
"""
from pathlib import Path
from typing import Optional

def encrypt_api_key(plaintext: str, data_dir: Optional[Path] = None) -> str:
    if not plaintext:
        return ""
    try:
        from src.secure_settings import store_api_key
        store_api_key(plaintext)
    except Exception:
        pass
    # Return value is no longer used — caller should not store api_key_enc
    return plaintext

def decrypt_api_key(ciphertext: str, data_dir: Optional[Path] = None) -> str:
    try:
        from src.secure_settings import read_api_key
        key = read_api_key()
        return key or ""
    except Exception:
        return ""
