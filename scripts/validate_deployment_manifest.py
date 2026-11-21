#!/usr/bin/env python3
"""Validate Kolo contract deployment manifests.

A manifest records the public facts of a contract deployment: network,
contract ID, Wasm hash, deployer, source commit, and the admin and token
chosen at initialization. Validation runs in two stages:

* Offline (always): structure, value formats, StrKey checksums, network
  passphrase, non-production marking, Stellar Asset Contract IDs, and a
  scan for secret material.
* Online (``--online``): every on-chain claim is re-derived from public
  Horizon and Soroban RPC responses, so a reviewer does not have to trust
  the file or whoever wrote it.

The script only reads public endpoints and needs no keys. It uses the
Python standard library only. See deployments/README.md for the format.

Exit codes: 0 valid, 1 invalid, 2 usage error, 3 network unreachable.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import datetime as dt
import hashlib
import json
import re
import struct
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Dict, Iterator, List, Optional, Sequence, TextIO, Tuple

MANIFEST_VERSION = 1

NETWORK_PASSPHRASES = {
    "testnet": "Test SDF Network ; September 2015",
    "futurenet": "Test SDF Future Network ; October 2022",
}
MAINNET_PASSPHRASE = "Public Global Stellar Network ; September 2015"

PROVENANCE_LEVELS = ("reproducible-build", "interface-match", "unverified")
TOKEN_KINDS = ("stellar-asset-contract", "contract-token")
STELLAR_ASSET_DECIMALS = 7

# (required, optional) fields for every object in the format. Unknown fields
# are rejected so that something like "deployer_secret" cannot be added
# quietly. Keep this in sync with deployments/manifest.v1.schema.json; the
# tests compare the two.
OBJECT_FIELDS: Dict[str, Tuple[frozenset, frozenset]] = {
    "manifest": (
        frozenset({"manifest_version", "production", "notice", "network", "contract",
                   "source", "deployment", "initialization"}),
        frozenset({"$schema", "last_verified"}),
    ),
    "network": (
        frozenset({"name", "passphrase", "rpc_url", "horizon_url"}),
        frozenset({"explorer_url"}),
    ),
    "contract": (
        frozenset({"name", "contract_id", "wasm_hash"}),
        frozenset({"wasm_size_bytes"}),
    ),
    "source": (
        frozenset({"repository", "commit", "path", "provenance"}),
        frozenset({"toolchain", "notes"}),
    ),
    "source.toolchain": (
        frozenset(),
        frozenset({"rustc", "soroban_sdk", "target", "optimizer"}),
    ),
    "deployment": (
        frozenset({"deployer", "deployed_at", "ledger", "upload_transaction",
                   "deploy_transaction"}),
        frozenset(),
    ),
    "initialization": (
        frozenset({"status", "admin", "token", "transaction"}),
        frozenset(),
    ),
    "initialization.token": (
        frozenset({"kind", "contract_id", "code", "issuer", "decimals"}),
        frozenset(),
    ),
    "last_verified": (
        frozenset({"date", "ledger"}),
        frozenset(),
    ),
}

HASH_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
TIMESTAMP_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ASSET_CODE_RE = re.compile(r"^[A-Za-z0-9]{1,12}$")

# A Stellar secret seed is "S" followed by 55 base32 characters. The
# lookarounds stop the pattern from matching inside a longer base32 run.
SECRET_SEED_RE = re.compile(r"(?<![A-Z2-7])S[A-Z2-7]{55}(?![A-Z2-7])")
PRIVATE_KEY_BLOCK_RE = re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")
SECRET_FIELD_RE = re.compile(r"secret|private|mnemonic|seed|signing", re.IGNORECASE)

USER_AGENT = "kolo-deployment-manifest-validator/1"


class ManifestLoadError(Exception):
    """The file could not be read as a manifest at all."""


class NetworkUnavailable(Exception):
    """A public endpoint could not be reached or returned an unusable reply."""


class Report:
    """Collects the outcome of every check made against one manifest."""

    def __init__(self) -> None:
        self.errors: List[str] = []
        self.warnings: List[str] = []
        self.verified: List[str] = []

    def error(self, path: str, message: str) -> None:
        self.errors.append(f"{path}: {message}")

    def warn(self, path: str, message: str) -> None:
        self.warnings.append(f"{path}: {message}")

    def ok(self, message: str) -> None:
        self.verified.append(message)

    @property
    def passed(self) -> bool:
        return not self.errors


# ---------------------------------------------------------------------------
# StrKey (SEP-23) encoding

STRKEY_VERSION = {"account": 6 << 3, "contract": 2 << 3}


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if crc & 0x8000 else (crc << 1)
            crc &= 0xFFFF
    return crc


def decode_strkey(value: Any, kind: str) -> bytes:
    """Return the 32-byte payload of a StrKey, or raise ValueError."""
    if not isinstance(value, str) or len(value) != 56:
        raise ValueError("must be a 56-character StrKey")
    try:
        raw = base64.b32decode(value)
    except (binascii.Error, ValueError):
        raise ValueError("is not valid base32") from None
    if raw[0] != STRKEY_VERSION[kind]:
        prefix = "G" if kind == "account" else "C"
        raise ValueError(f"must be a {kind} StrKey starting with {prefix}")
    body, checksum = raw[:-2], raw[-2:]
    if struct.unpack("<H", checksum)[0] != crc16_xmodem(body):
        raise ValueError("has an invalid checksum (check for a typo)")
    return body[1:]


def encode_strkey(payload: bytes, kind: str) -> str:
    body = bytes([STRKEY_VERSION[kind]]) + payload
    return base64.b32encode(body + struct.pack("<H", crc16_xmodem(body))).decode()


# ---------------------------------------------------------------------------
# The handful of XDR structures needed to check a deployment

SCV_SYMBOL = 15
SCV_VEC = 16
SCV_ADDRESS = 18
SCV_CONTRACT_INSTANCE = 19
SCV_LEDGER_KEY_CONTRACT_INSTANCE = 20
LEDGER_ENTRY_CONTRACT_DATA = 6
LEDGER_ENTRY_CONTRACT_CODE = 7
DURABILITY_PERSISTENT = 1
SC_ADDRESS_ACCOUNT = 0
SC_ADDRESS_CONTRACT = 1
EXECUTABLE_WASM = 0
OP_INVOKE_HOST_FUNCTION = 24
HOST_FN_CREATE_CONTRACT = 1
HOST_FN_UPLOAD_WASM = 2
HOST_FN_CREATE_CONTRACT_V2 = 3
ENVELOPE_TYPE_CONTRACT_ID = 8
PREIMAGE_FROM_ADDRESS = 0
PREIMAGE_FROM_ASSET = 1


def u32(value: int) -> bytes:
    return struct.pack(">I", value)


def read_u32(data: bytes, offset: int) -> int:
    if offset + 4 > len(data):
        raise ValueError("XDR ended early")
    return struct.unpack_from(">I", data, offset)[0]


def account_address(public_key: bytes) -> bytes:
    # ScAddress::Account(AccountId(PublicKey::Ed25519(key)))
    return u32(SC_ADDRESS_ACCOUNT) + u32(0) + public_key


def contract_address(contract_hash: bytes) -> bytes:
    return u32(SC_ADDRESS_CONTRACT) + contract_hash


def instance_ledger_key(contract_hash: bytes) -> bytes:
    return (u32(LEDGER_ENTRY_CONTRACT_DATA) + contract_address(contract_hash)
            + u32(SCV_LEDGER_KEY_CONTRACT_INSTANCE) + u32(DURABILITY_PERSISTENT))


def code_ledger_key(wasm_hash: bytes) -> bytes:
    return u32(LEDGER_ENTRY_CONTRACT_CODE) + wasm_hash


def storage_key(variant: str) -> bytes:
    """ScVal for a unit variant of the contract's DataKey enum: Vec[Symbol]."""
    name = variant.encode()
    padding = b"\0" * (-len(name) % 4)
    return u32(SCV_VEC) + u32(1) + u32(1) + u32(SCV_SYMBOL) + u32(len(name)) + name + padding


def parse_instance_entry(data: bytes, contract_hash: bytes) -> Tuple[bytes, Optional[bytes]]:
    """Return (wasm hash, instance storage XDR or None) from a contract instance entry."""
    expected = (u32(LEDGER_ENTRY_CONTRACT_DATA) + u32(0) + contract_address(contract_hash)
                + u32(SCV_LEDGER_KEY_CONTRACT_INSTANCE) + u32(DURABILITY_PERSISTENT)
                + u32(SCV_CONTRACT_INSTANCE))
    if not data.startswith(expected):
        raise ValueError("ledger entry is not this contract's instance")
    offset = len(expected)
    if read_u32(data, offset) != EXECUTABLE_WASM:
        raise ValueError("contract is not backed by Wasm (it may be a Stellar Asset Contract)")
    wasm_hash = data[offset + 4:offset + 36]
    if len(wasm_hash) != 32:
        raise ValueError("XDR ended early")
    has_storage = read_u32(data, offset + 36)
    return wasm_hash, (data[offset + 40:] if has_storage else None)


def parse_code_entry(data: bytes) -> bytes:
    """Return the Wasm bytes held by a contract code entry."""
    if read_u32(data, 0) != LEDGER_ENTRY_CONTRACT_CODE:
        raise ValueError("ledger entry is not contract code")
    offset = 8
    if read_u32(data, 4) == 1:
        # ContractCodeEntry v1 extension: ExtensionPoint plus
        # ContractCodeCostInputs (its own ExtensionPoint and ten u32 counters).
        offset += 4 + 4 + 10 * 4
    offset += 32
    length = read_u32(data, offset)
    code = data[offset + 4:offset + 4 + length]
    if len(code) != length:
        raise ValueError("XDR ended early")
    return code


def find_create_contract(envelope: bytes, deployer_key: bytes) -> List[Tuple[bytes, bytes]]:
    """Find (salt, wasm hash) for each create-contract operation by the deployer.

    Matches InvokeHostFunction ops whose host function is CreateContract (or
    CreateContractV2, which only appends constructor arguments) with a
    contract ID preimage derived from the deployer's account.
    """
    found = []
    for host_fn in (HOST_FN_CREATE_CONTRACT, HOST_FN_CREATE_CONTRACT_V2):
        prefix = (u32(OP_INVOKE_HOST_FUNCTION) + u32(host_fn) + u32(PREIMAGE_FROM_ADDRESS)
                  + account_address(deployer_key))
        start = envelope.find(prefix)
        while start != -1:
            cursor = start + len(prefix)
            salt = envelope[cursor:cursor + 32]
            executable = envelope[cursor + 32:cursor + 68]
            if len(executable) == 36 and executable[:4] == u32(EXECUTABLE_WASM):
                found.append((salt, executable[4:]))
            start = envelope.find(prefix, start + 1)
    return found


def find_uploaded_wasm(envelope: bytes) -> List[bytes]:
    """Return the Wasm payload of each upload operation in a transaction envelope."""
    prefix = u32(OP_INVOKE_HOST_FUNCTION) + u32(HOST_FN_UPLOAD_WASM)
    found = []
    start = envelope.find(prefix)
    while start != -1:
        length = read_u32(envelope, start + len(prefix))
        code = envelope[start + len(prefix) + 4:start + len(prefix) + 4 + length]
        if len(code) == length and code.startswith(b"\0asm"):
            found.append(code)
        start = envelope.find(prefix, start + 1)
    return found


def derive_contract_id(passphrase: str, preimage: bytes) -> str:
    network_id = hashlib.sha256(passphrase.encode()).digest()
    digest = hashlib.sha256(u32(ENVELOPE_TYPE_CONTRACT_ID) + network_id + preimage).digest()
    return encode_strkey(digest, "contract")


def deployed_contract_id(passphrase: str, deployer_key: bytes, salt: bytes) -> str:
    preimage = u32(PREIMAGE_FROM_ADDRESS) + account_address(deployer_key) + salt
    return derive_contract_id(passphrase, preimage)


def asset_contract_id(passphrase: str, code: str, issuer_key: Optional[bytes]) -> str:
    """Contract ID of the Stellar Asset Contract for a classic asset."""
    if issuer_key is None:
        asset = u32(0)  # ASSET_TYPE_NATIVE
    else:
        width, asset_type = (4, 1) if len(code) <= 4 else (12, 2)
        asset = u32(asset_type) + code.encode().ljust(width, b"\0") + u32(0) + issuer_key
    return derive_contract_id(passphrase, u32(PREIMAGE_FROM_ASSET) + asset)


# ---------------------------------------------------------------------------
# Loading and offline checks


def _reject_duplicate_keys(pairs: List[Tuple[str, Any]]) -> Dict[str, Any]:
    result: Dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            # Parsers disagree on which duplicate wins, so a reviewer and a
            # tool could read different values from the same file.
            raise ManifestLoadError(f"duplicate field {key!r}")
        result[key] = value
    return result


def _reject_constant(name: str) -> None:
    raise ManifestLoadError(f"{name} is not valid JSON")


def load_manifest(text: str) -> Any:
    try:
        return json.loads(text, object_pairs_hook=_reject_duplicate_keys,
                          parse_constant=_reject_constant)
    except json.JSONDecodeError as exc:
        raise ManifestLoadError(f"invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}") from None


def scan_for_secrets(text: str, manifest: Any, report: Report) -> None:
    """Flag anything that looks like key material. Matched values are never echoed."""
    for line_number, line in enumerate(text.splitlines(), start=1):
        if SECRET_SEED_RE.search(line):
            report.error(f"line {line_number}", "contains what looks like a Stellar secret seed (S...); "
                         "remove it and rotate that key")
        if PRIVATE_KEY_BLOCK_RE.search(line):
            report.error(f"line {line_number}", "contains a private key block; remove it and rotate that key")

    for path, key in _walk_keys(manifest, ""):
        if SECRET_FIELD_RE.search(key):
            report.error(path, f"field name {key!r} suggests secret material, which manifests must not hold")


def _walk_keys(value: Any, path: str) -> Iterator[Tuple[str, str]]:
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else key
            yield child_path, key
            yield from _walk_keys(child, child_path)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from _walk_keys(child, f"{path}[{index}]")


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _check_object(report: Report, path: str, value: Any) -> bool:
    if not isinstance(value, dict):
        report.error(path, "must be an object")
        return False
    required, optional = OBJECT_FIELDS[path]
    for missing in sorted(required - value.keys()):
        report.error(path, f"missing required field {missing!r}")
    for unknown in sorted(value.keys() - required - optional):
        report.error(path, f"unknown field {unknown!r}")
    return True


def _check_strkey(report: Report, path: str, value: Any, kind: str) -> Optional[bytes]:
    try:
        return decode_strkey(value, kind)
    except ValueError as exc:
        report.error(path, str(exc))
        return None


def _check_hash(report: Report, path: str, value: Any) -> None:
    if not isinstance(value, str) or not HASH_RE.match(value):
        report.error(path, "must be 64 lowercase hex characters")


def _check_https_url(report: Report, path: str, value: Any) -> None:
    if not isinstance(value, str):
        report.error(path, "must be a URL string")
        return
    parsed = urllib.parse.urlsplit(value)
    if parsed.scheme != "https" or not parsed.hostname:
        report.error(path, "must be an https:// URL")
    elif parsed.username or parsed.password:
        report.error(path, "must not embed credentials")


def _check_non_empty_string(report: Report, path: str, value: Any) -> None:
    if not isinstance(value, str) or not value.strip():
        report.error(path, "must be a non-empty string")


def _parse_timestamp(report: Report, path: str, value: Any) -> Optional[dt.datetime]:
    if isinstance(value, str) and TIMESTAMP_RE.match(value):
        try:
            return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
        except ValueError:
            pass
    report.error(path, "must be a UTC timestamp like 2026-01-31T12:00:00Z")
    return None


def check_network(report: Report, network: Any) -> None:
    if not _check_object(report, "network", network):
        return
    name, passphrase = network.get("name"), network.get("passphrase")
    if name == "mainnet" or passphrase == MAINNET_PASSPHRASE:
        report.error("network", "Mainnet (production) deployments are out of scope for manifest version 1")
    elif name not in NETWORK_PASSPHRASES:
        report.error("network.name", f"must be one of {', '.join(sorted(NETWORK_PASSPHRASES))}")
    elif passphrase != NETWORK_PASSPHRASES[name]:
        report.error("network.passphrase", f"does not match the {name} passphrase "
                     f"{NETWORK_PASSPHRASES[name]!r}")
    for url_field in ("rpc_url", "horizon_url", "explorer_url"):
        if url_field in network:
            _check_https_url(report, f"network.{url_field}", network[url_field])


def check_contract(report: Report, contract: Any) -> None:
    if not _check_object(report, "contract", contract):
        return
    _check_non_empty_string(report, "contract.name", contract.get("name"))
    _check_strkey(report, "contract.contract_id", contract.get("contract_id"), "contract")
    _check_hash(report, "contract.wasm_hash", contract.get("wasm_hash"))
    if "wasm_size_bytes" in contract:
        size = contract["wasm_size_bytes"]
        if not _is_int(size) or size < 1:
            report.error("contract.wasm_size_bytes", "must be a positive integer")


def check_source(report: Report, source: Any) -> None:
    if not _check_object(report, "source", source):
        return
    _check_https_url(report, "source.repository", source.get("repository"))
    commit = source.get("commit")
    if not isinstance(commit, str) or not COMMIT_RE.match(commit):
        report.error("source.commit", "must be a full 40-character lowercase commit SHA")
    path = source.get("path")
    if not isinstance(path, str) or not path or path.startswith("/") or ".." in path.split("/"):
        report.error("source.path", "must be a relative path inside the repository")
    if source.get("provenance") not in PROVENANCE_LEVELS:
        report.error("source.provenance", f"must be one of {', '.join(PROVENANCE_LEVELS)}")
    if "toolchain" in source and _check_object(report, "source.toolchain", source["toolchain"]):
        for key, value in source["toolchain"].items():
            _check_non_empty_string(report, f"source.toolchain.{key}", value)
    if "notes" in source:
        _check_non_empty_string(report, "source.notes", source["notes"])


def check_deployment(report: Report, deployment: Any) -> Optional[dt.datetime]:
    if not _check_object(report, "deployment", deployment):
        return None
    _check_strkey(report, "deployment.deployer", deployment.get("deployer"), "account")
    deployed_at = _parse_timestamp(report, "deployment.deployed_at", deployment.get("deployed_at"))
    ledger = deployment.get("ledger")
    if not _is_int(ledger) or ledger < 1:
        report.error("deployment.ledger", "must be a positive integer")
    if deployment.get("upload_transaction") is not None:
        _check_hash(report, "deployment.upload_transaction", deployment["upload_transaction"])
    _check_hash(report, "deployment.deploy_transaction", deployment.get("deploy_transaction"))
    return deployed_at


def check_token(report: Report, token: Any, passphrase: Optional[str]) -> None:
    path = "initialization.token"
    if not _check_object(report, path, token):
        return
    kind = token.get("kind")
    if kind not in TOKEN_KINDS:
        report.error(f"{path}.kind", f"must be one of {', '.join(TOKEN_KINDS)}")
    contract_key = _check_strkey(report, f"{path}.contract_id", token.get("contract_id"), "contract")

    code = token.get("code")
    if not isinstance(code, str) or not ASSET_CODE_RE.match(code):
        report.error(f"{path}.code", "must be 1-12 letters or digits")
        code = None
    issuer = token.get("issuer")
    issuer_key = None
    if issuer is not None:
        issuer_key = _check_strkey(report, f"{path}.issuer", issuer, "account")
    elif code is not None and code != "XLM":
        report.error(f"{path}.issuer", "is required for any asset other than native XLM; "
                     "an asset code alone does not identify a Stellar asset")

    decimals = token.get("decimals")
    if not _is_int(decimals) or not 0 <= decimals <= 38:
        report.error(f"{path}.decimals", "must be an integer from 0 to 38")

    if kind != "stellar-asset-contract":
        return
    if _is_int(decimals) and decimals != STELLAR_ASSET_DECIMALS:
        report.error(f"{path}.decimals", "Stellar Asset Contracts always use 7 decimals")
    # A Stellar Asset Contract's ID is a pure function of the asset and the
    # network, so a wrong token address can be caught without any lookup.
    issuer_ok = issuer is None or issuer_key is not None
    if passphrase in NETWORK_PASSPHRASES.values() and code and issuer_ok and contract_key:
        expected = asset_contract_id(passphrase, code, issuer_key)
        if token["contract_id"] != expected:
            asset = "native XLM" if issuer is None else f"{code} issued by {issuer}"
            report.error(f"{path}.contract_id", f"is not the Stellar Asset Contract for {asset} "
                         f"on this network (expected {expected})")


def check_initialization(report: Report, initialization: Any, passphrase: Optional[str]) -> None:
    if not _check_object(report, "initialization", initialization):
        return
    status = initialization.get("status")
    if status == "uninitialized":
        for key in ("admin", "token", "transaction"):
            if initialization.get(key) is not None:
                report.error(f"initialization.{key}", "must be null while the contract is uninitialized")
    elif status == "initialized":
        _check_strkey(report, "initialization.admin", initialization.get("admin"), "account")
        check_token(report, initialization.get("token"), passphrase)
        _check_hash(report, "initialization.transaction", initialization.get("transaction"))
    else:
        report.error("initialization.status", "must be 'uninitialized' or 'initialized'")


def check_last_verified(report: Report, manifest: Dict[str, Any], deployed_at: Optional[dt.datetime]) -> None:
    last_verified = manifest["last_verified"]
    if not _check_object(report, "last_verified", last_verified):
        return
    date = last_verified.get("date")
    try:
        if not isinstance(date, str) or not DATE_RE.match(date):
            raise ValueError
        verified_on = dt.datetime.strptime(date, "%Y-%m-%d").date()
    except ValueError:
        report.error("last_verified.date", "must be a date like 2026-01-31")
    else:
        if deployed_at and verified_on < deployed_at.date():
            report.error("last_verified.date", "is earlier than deployment.deployed_at")

    ledger = last_verified.get("ledger")
    deployment = manifest.get("deployment")
    deployed_ledger = deployment.get("ledger") if isinstance(deployment, dict) else None
    if not _is_int(ledger) or ledger < 1:
        report.error("last_verified.ledger", "must be a positive integer")
    elif _is_int(deployed_ledger) and ledger < deployed_ledger:
        report.error("last_verified.ledger", "is earlier than deployment.ledger")


def check_offline(manifest: Any, report: Report) -> None:
    if not _check_object(report, "manifest", manifest):
        return
    if manifest.get("manifest_version") != MANIFEST_VERSION or isinstance(manifest.get("manifest_version"), bool):
        report.error("manifest.manifest_version", f"must be {MANIFEST_VERSION}")
    if manifest.get("production") is not False:
        report.error("manifest.production", "must be false; version 1 manifests describe "
                     "non-production deployments only")
    _check_non_empty_string(report, "manifest.notice", manifest.get("notice"))

    network = manifest.get("network")
    check_network(report, network)
    passphrase = network.get("passphrase") if isinstance(network, dict) else None
    check_contract(report, manifest.get("contract"))
    check_source(report, manifest.get("source"))
    deployed_at = check_deployment(report, manifest.get("deployment"))
    check_initialization(report, manifest.get("initialization"), passphrase)
    if "last_verified" in manifest:
        check_last_verified(report, manifest, deployed_at)


# ---------------------------------------------------------------------------
# Online checks


class PublicNetworkClient:
    """Read-only access to a network's public Horizon and Soroban RPC endpoints."""

    def __init__(self, rpc_url: str, horizon_url: str, timeout: float = 30.0) -> None:
        self.rpc_url = rpc_url
        self.horizon_url = horizon_url.rstrip("/")
        self.timeout = timeout

    def _fetch(self, request: urllib.request.Request) -> Optional[Any]:
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            raise NetworkUnavailable(f"{request.full_url} returned HTTP {exc.code}") from None
        except (urllib.error.URLError, OSError) as exc:
            raise NetworkUnavailable(f"could not reach {request.full_url}: {exc}") from None
        except json.JSONDecodeError:
            raise NetworkUnavailable(f"{request.full_url} did not return JSON") from None

    def rpc(self, method: str, params: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        body: Dict[str, Any] = {"jsonrpc": "2.0", "id": 1, "method": method}
        if params is not None:
            body["params"] = params
        request = urllib.request.Request(
            self.rpc_url,
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json", "User-Agent": USER_AGENT},
        )
        response = self._fetch(request)
        if not isinstance(response, dict) or "result" not in response:
            detail = response.get("error") if isinstance(response, dict) else None
            raise NetworkUnavailable(f"RPC {method} failed: {detail or 'no result'}")
        return response["result"]

    def network_passphrase(self) -> Optional[str]:
        return self.rpc("getNetwork").get("passphrase")

    def ledger_entry(self, key: bytes) -> Tuple[Optional[Dict[str, Any]], int]:
        """Return (entry or None, latest ledger) for one ledger key."""
        result = self.rpc("getLedgerEntries", {"keys": [base64.b64encode(key).decode()]})
        entries = result.get("entries") or []
        return (entries[0] if entries else None), int(result.get("latestLedger", 0))

    def transaction(self, tx_hash: str) -> Optional[Dict[str, Any]]:
        request = urllib.request.Request(f"{self.horizon_url}/transactions/{tx_hash}",
                                         headers={"Accept": "application/json", "User-Agent": USER_AGENT})
        return self._fetch(request)


def _check_ttl(report: Report, path: str, what: str, entry: Dict[str, Any], latest_ledger: int) -> None:
    live_until = int(entry.get("liveUntilLedgerSeq") or 0)
    if live_until < latest_ledger:
        report.warn(path, f"{what} is not live (liveUntilLedgerSeq {live_until}, latest ledger "
                    f"{latest_ledger}); restore it with `stellar contract restore` before invoking the contract")


def _successful_transaction(report: Report, client: PublicNetworkClient, path: str, tx_hash: str,
                            deployer: Optional[str]) -> Optional[Dict[str, Any]]:
    tx = client.transaction(tx_hash)
    if tx is None:
        report.error(path, f"transaction {tx_hash} was not found on this network")
        return None
    if tx.get("successful") is not True:
        report.error(path, f"transaction {tx_hash} did not succeed")
        return None
    if deployer is not None and tx.get("source_account") != deployer:
        report.error(path, f"transaction was sent by {tx.get('source_account')}, not the deployer {deployer}")
        return None
    return tx


def _verify_deploy_transaction(report: Report, client: PublicNetworkClient, manifest: Dict[str, Any]) -> None:
    deployment, contract = manifest["deployment"], manifest["contract"]
    deployer = deployment["deployer"]
    tx = _successful_transaction(report, client, "deployment.deploy_transaction",
                                 deployment["deploy_transaction"], deployer)
    if tx is None:
        return
    if tx.get("ledger") != deployment["ledger"]:
        report.error("deployment.ledger", f"deploy transaction is in ledger {tx.get('ledger')}")
    if tx.get("created_at") != deployment["deployed_at"]:
        report.error("deployment.deployed_at", f"deploy transaction closed at {tx.get('created_at')}")

    envelope = base64.b64decode(tx.get("envelope_xdr") or "")
    candidates = find_create_contract(envelope, decode_strkey(deployer, "account"))
    passphrase = manifest["network"]["passphrase"]
    derived = {(deployed_contract_id(passphrase, decode_strkey(deployer, "account"), salt), wasm_hash.hex())
               for salt, wasm_hash in candidates}
    if (contract["contract_id"], contract["wasm_hash"]) in derived:
        report.ok(f"deploy transaction by {deployer} derives contract ID {contract['contract_id']} "
                  f"with Wasm {contract['wasm_hash']}")
    elif derived:
        found = ", ".join(f"{cid} (Wasm {wasm})" for cid, wasm in sorted(derived))
        report.error("deployment.deploy_transaction", f"creates {found}, not the manifest's contract")
    else:
        report.error("deployment.deploy_transaction", "does not create a Wasm contract from the deployer's address")


def _verify_upload_transaction(report: Report, client: PublicNetworkClient, manifest: Dict[str, Any]) -> None:
    tx_hash = manifest["deployment"]["upload_transaction"]
    if tx_hash is None:
        return
    tx = _successful_transaction(report, client, "deployment.upload_transaction", tx_hash,
                                 manifest["deployment"]["deployer"])
    if tx is None:
        return
    uploaded = find_uploaded_wasm(base64.b64decode(tx.get("envelope_xdr") or ""))
    hashes = {hashlib.sha256(code).hexdigest() for code in uploaded}
    if manifest["contract"]["wasm_hash"] in hashes:
        report.ok(f"upload transaction {tx_hash} carries Wasm hashing to {manifest['contract']['wasm_hash']}")
    else:
        report.error("deployment.upload_transaction", "does not upload Wasm matching contract.wasm_hash")


def _verify_instance(report: Report, client: PublicNetworkClient, manifest: Dict[str, Any]) -> None:
    contract, initialization = manifest["contract"], manifest["initialization"]
    contract_hash = decode_strkey(contract["contract_id"], "contract")
    entry, latest = client.ledger_entry(instance_ledger_key(contract_hash))
    if entry is None:
        report.error("contract.contract_id", "no contract instance exists on this network")
        return
    try:
        wasm_hash, storage = parse_instance_entry(base64.b64decode(entry["xdr"]), contract_hash)
    except (ValueError, KeyError, binascii.Error) as exc:
        report.error("contract.contract_id", f"could not read the contract instance: {exc}")
        return
    if wasm_hash.hex() != contract["wasm_hash"]:
        report.error("contract.wasm_hash", f"the contract instance runs Wasm {wasm_hash.hex()}")
    else:
        report.ok(f"contract instance runs Wasm {wasm_hash.hex()}")
    _check_ttl(report, "contract.contract_id", "contract instance", entry, latest)

    storage = storage or b""
    admin_key = storage_key("Admin")
    if initialization["status"] == "uninitialized":
        if admin_key in storage:
            report.error("initialization.status", "the contract has been initialized on-chain")
        else:
            report.ok("contract instance has no admin set (uninitialized)")
        return

    admin = initialization["admin"]
    token_id = initialization["token"]["contract_id"]
    expected_admin = admin_key + u32(SCV_ADDRESS) + account_address(decode_strkey(admin, "account"))
    expected_token = (storage_key("Token") + u32(SCV_ADDRESS)
                      + contract_address(decode_strkey(token_id, "contract")))
    if admin_key not in storage:
        report.error("initialization.status", "the contract instance has no admin; it is not initialized")
        return
    if expected_admin in storage:
        report.ok(f"contract instance stores admin {admin}")
    else:
        report.error("initialization.admin", "does not match the admin stored in the contract instance")
    if expected_token in storage:
        report.ok(f"contract instance stores token {token_id}")
    else:
        report.error("initialization.token.contract_id",
                     "does not match the token stored in the contract instance")
    if _successful_transaction(report, client, "initialization.transaction",
                               initialization["transaction"], None) is not None:
        report.ok(f"initialization transaction {initialization['transaction']} succeeded")


def _verify_code(report: Report, client: PublicNetworkClient, contract: Dict[str, Any]) -> None:
    entry, latest = client.ledger_entry(code_ledger_key(bytes.fromhex(contract["wasm_hash"])))
    if entry is None:
        report.error("contract.wasm_hash", "no Wasm with this hash is stored on this network")
        return
    try:
        code = parse_code_entry(base64.b64decode(entry["xdr"]))
    except (ValueError, KeyError, binascii.Error) as exc:
        report.error("contract.wasm_hash", f"could not read the contract code entry: {exc}")
        return
    if hashlib.sha256(code).hexdigest() != contract["wasm_hash"]:
        report.error("contract.wasm_hash", "the stored Wasm bytes do not hash to this value")
    else:
        report.ok(f"stored Wasm ({len(code)} bytes) hashes to {contract['wasm_hash']}")
    if "wasm_size_bytes" in contract and contract["wasm_size_bytes"] != len(code):
        report.error("contract.wasm_size_bytes", f"stored Wasm is {len(code)} bytes")
    _check_ttl(report, "contract.wasm_hash", "contract code", entry, latest)


def check_online(manifest: Dict[str, Any], report: Report, client: PublicNetworkClient) -> None:
    """Re-derive the manifest's on-chain claims. Assumes check_offline passed."""
    expected = manifest["network"]["passphrase"]
    actual = client.network_passphrase()
    if actual != expected:
        # Every later answer would come from the wrong network.
        report.error("network.rpc_url", f"reports passphrase {actual!r}, expected {expected!r}")
        return
    report.ok(f"RPC endpoint reports the {manifest['network']['name']} passphrase")

    _verify_deploy_transaction(report, client, manifest)
    _verify_upload_transaction(report, client, manifest)
    _verify_instance(report, client, manifest)
    _verify_code(report, client, manifest["contract"])


# ---------------------------------------------------------------------------
# Command line


def validate_file(path: Path, online: bool, timeout: float) -> Report:
    report = Report()
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        report.error(str(path), f"could not read file: {exc}")
        return report

    try:
        manifest = load_manifest(text)
    except ManifestLoadError as exc:
        report.error("manifest", str(exc))
        manifest = None

    # Scan even when the JSON is broken: a pasted key is still a leak.
    scan_for_secrets(text, manifest, report)
    if manifest is None:
        return report
    check_offline(manifest, report)
    if online and report.passed:
        network = manifest["network"]
        client = PublicNetworkClient(network["rpc_url"], network["horizon_url"], timeout)
        check_online(manifest, report, client)
    return report


def print_report(path: Path, report: Report, stream: Optional[TextIO] = None) -> None:
    stream = stream or sys.stdout
    print(f"{'PASS' if report.passed else 'FAIL'}  {path}", file=stream)
    for line in report.verified:
        print(f"  verified  {line}", file=stream)
    for line in report.warnings:
        print(f"  warning   {line}", file=stream)
    for line in report.errors:
        print(f"  error     {line}", file=stream)


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate Kolo contract deployment manifests (see deployments/README.md).")
    parser.add_argument("manifests", nargs="+", type=Path, metavar="MANIFEST")
    parser.add_argument("--online", action="store_true",
                        help="also verify every on-chain claim against the manifest's public "
                             "Horizon and Soroban RPC endpoints")
    parser.add_argument("--timeout", type=float, default=30.0,
                        help="seconds to wait for each network request (default: 30)")
    args = parser.parse_args(argv)

    exit_code = 0
    for path in args.manifests:
        try:
            report = validate_file(path, args.online, args.timeout)
        except NetworkUnavailable as exc:
            print(f"ERROR {path}\n  could not complete online verification: {exc}", file=sys.stderr)
            exit_code = max(exit_code, 3)
            continue
        print_report(path, report)
        if not report.passed:
            exit_code = max(exit_code, 1)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
