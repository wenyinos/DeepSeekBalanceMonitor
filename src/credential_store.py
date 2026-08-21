"""
Deprecated: Windows Credential Manager storage.

v2.0 removed this module. All API keys are now stored via
src/secure_settings.py (Fernet + SQLite). This file is kept
as a stub to avoid ImportError for any stale imports — it
does nothing.
"""

def store_credential(api_key: str):
    return

def read_credential():
    return None

def delete_credential():
    return
