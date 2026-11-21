"""Tests for validate_deployment_manifest.py.

Run from the repository root:

    python3 -m unittest discover -s scripts -p "test_*.py"
"""

from __future__ import annotations

import base64
import copy
import hashlib
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import validate_deployment_manifest as vdm
from validate_deployment_manifest import u32

REPO_ROOT = Path(__file__).resolve().parent.parent
TESTNET_MANIFEST = REPO_ROOT / "deployments" / "testnet" / "kolo-savings-group.json"
SCHEMA = REPO_ROOT / "deployments" / "manifest.v1.schema.json"
RECORDED = json.loads((Path(__file__).parent / "testdata" / "testnet-deployment-responses.json")
                      .read_text(encoding="utf-8"))

TESTNET = vdm.NETWORK_PASSPHRASES["testnet"]
DEPLOYER = "GCEFQRSJTT5KWNZTMXK7GXSC3MC2LHMFW3YRDBKQK2UBIRIGX4KQRHLU"
CONTRACT_ID = "CBQCB6KIXAPDPKGNZFR3KSBBJNEJRFN3MAQL7S46JGFA5I22AUL4OPYL"
WASM_HASH = "f68a30e74126832be3e8530d667c27f212005a8bbb3de0d096cb869bf46d3911"

# Well-known Stellar Asset Contract IDs, used as fixed points for derivation.
TESTNET_XLM_SAC = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
TESTNET_USDC_ISSUER = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
TESTNET_USDC_SAC = "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA"

# Shaped like a secret seed but built at runtime, so no seed-like literal
# sits in the repository for secret scanners to trip over.
FAKE_SEED = "S" + "A" * 55


def load_testnet_manifest() -> dict:
    return json.loads(TESTNET_MANIFEST.read_text(encoding="utf-8"))


def offline_errors(manifest, text=None) -> list:
    report = vdm.Report()
    vdm.scan_for_secrets(text if text is not None else json.dumps(manifest, indent=2), manifest, report)
    vdm.check_offline(manifest, report)
    return report.errors


def flip_last_char(strkey: str) -> str:
    return strkey[:-1] + ("A" if strkey[-1] != "A" else "B")


def initialized(manifest: dict, token: dict, admin: str = TESTNET_USDC_ISSUER) -> dict:
    manifest["initialization"] = {
        "status": "initialized",
        "admin": admin,
        "token": token,
        "transaction": "ab" * 32,
    }
    return manifest


def usdc_token(**overrides) -> dict:
    token = {
        "kind": "stellar-asset-contract",
        "contract_id": TESTNET_USDC_SAC,
        "code": "USDC",
        "issuer": TESTNET_USDC_ISSUER,
        "decimals": 7,
    }
    token.update(overrides)
    return token


class CommittedManifestTests(unittest.TestCase):
    def test_testnet_manifest_passes_offline_checks(self):
        report = vdm.validate_file(TESTNET_MANIFEST, online=False, timeout=1)
        self.assertEqual(report.errors, [])

    def test_testnet_manifest_is_marked_non_production(self):
        manifest = load_testnet_manifest()
        self.assertIs(manifest["production"], False)
        self.assertEqual(manifest["network"]["name"], "testnet")

    def test_every_committed_manifest_is_valid(self):
        manifests = sorted((REPO_ROOT / "deployments").glob("*/*.json"))
        self.assertTrue(manifests)
        for path in manifests:
            with self.subTest(path=path.name):
                self.assertEqual(vdm.validate_file(path, online=False, timeout=1).errors, [])


class SchemaParityTests(unittest.TestCase):
    """The JSON Schema and the validator must describe the same fields."""

    def setUp(self):
        self.schema = json.loads(SCHEMA.read_text(encoding="utf-8"))

    def resolve(self, node):
        ref = node.get("$ref")
        if ref:
            return self.schema["$defs"][ref.rsplit("/", 1)[-1]]
        for option in node.get("oneOf", []):
            if option.get("type") != "null":
                return self.resolve(option)
        return node

    def test_fields_match_schema(self):
        nodes = {"manifest": self.schema}
        nodes["network"] = self.schema["properties"]["network"]
        nodes["contract"] = self.schema["properties"]["contract"]
        nodes["source"] = self.schema["properties"]["source"]
        nodes["source.toolchain"] = nodes["source"]["properties"]["toolchain"]
        nodes["deployment"] = self.schema["properties"]["deployment"]
        nodes["initialization"] = self.schema["properties"]["initialization"]
        nodes["initialization.token"] = self.resolve(nodes["initialization"]["properties"]["token"])
        nodes["last_verified"] = self.schema["properties"]["last_verified"]
        self.assertEqual(set(nodes), set(vdm.OBJECT_FIELDS))

        for path, node in nodes.items():
            with self.subTest(object=path):
                required, optional = vdm.OBJECT_FIELDS[path]
                self.assertFalse(node.get("additionalProperties", True))
                self.assertEqual(set(node.get("required", [])), set(required))
                self.assertEqual(set(node["properties"]), set(required | optional))

    def test_enums_match_schema(self):
        props = self.schema["properties"]
        self.assertEqual(set(props["network"]["properties"]["name"]["enum"]), set(vdm.NETWORK_PASSPHRASES))
        self.assertEqual(tuple(props["source"]["properties"]["provenance"]["enum"]), vdm.PROVENANCE_LEVELS)
        self.assertEqual(tuple(self.schema["$defs"]["token"]["properties"]["kind"]["enum"]), vdm.TOKEN_KINDS)


class SecretScanTests(unittest.TestCase):
    def test_seed_in_any_value_is_rejected_without_echoing_it(self):
        manifest = load_testnet_manifest()
        manifest["source"]["notes"] = f"deployed with {FAKE_SEED}"
        errors = offline_errors(manifest)
        self.assertTrue(any("secret seed" in error for error in errors))
        self.assertFalse(any(FAKE_SEED in error for error in errors))

    def test_seed_in_place_of_deployer_is_rejected(self):
        manifest = load_testnet_manifest()
        manifest["deployment"]["deployer"] = FAKE_SEED
        errors = offline_errors(manifest)
        self.assertTrue(any("secret seed" in error for error in errors))
        self.assertTrue(any(error.startswith("deployment.deployer:") for error in errors))

    def test_seed_is_caught_even_in_invalid_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.json"
            path.write_text('{"deployer_key": "' + FAKE_SEED + '",', encoding="utf-8")
            errors = vdm.validate_file(path, online=False, timeout=1).errors
        self.assertTrue(any("invalid JSON" in error for error in errors))
        self.assertTrue(any("secret seed" in error for error in errors))

    def test_secret_looking_field_name_is_rejected(self):
        manifest = load_testnet_manifest()
        manifest["deployment"]["signing_key"] = "redacted"
        errors = offline_errors(manifest)
        self.assertIn("deployment.signing_key: field name 'signing_key' suggests secret material, "
                      "which manifests must not hold", errors)

    def test_private_key_block_is_rejected(self):
        manifest = load_testnet_manifest()
        manifest["source"]["notes"] = "-----BEGIN OPENSSH PRIVATE KEY-----"
        self.assertTrue(any("private key block" in error for error in offline_errors(manifest)))

    def test_public_strkeys_are_not_mistaken_for_seeds(self):
        text = TESTNET_MANIFEST.read_text(encoding="utf-8")
        self.assertIsNone(vdm.SECRET_SEED_RE.search(text))

    def test_url_with_credentials_is_rejected(self):
        manifest = load_testnet_manifest()
        manifest["network"]["rpc_url"] = "https://user:hunter2@soroban-testnet.stellar.org"
        self.assertIn("network.rpc_url: must not embed credentials", offline_errors(manifest))


class StructureTests(unittest.TestCase):
    def setUp(self):
        self.manifest = load_testnet_manifest()

    def assert_error(self, expected_prefix):
        errors = offline_errors(self.manifest)
        self.assertTrue(any(error.startswith(expected_prefix) for error in errors),
                        f"expected an error starting with {expected_prefix!r}, got {errors}")

    def test_production_must_be_false(self):
        self.manifest["production"] = True
        self.assert_error("manifest.production:")

    def test_production_marker_is_required(self):
        del self.manifest["production"]
        self.assert_error("manifest: missing required field 'production'")

    def test_mainnet_is_out_of_scope(self):
        self.manifest["network"]["name"] = "mainnet"
        self.manifest["network"]["passphrase"] = vdm.MAINNET_PASSPHRASE
        self.assert_error("network: Mainnet (production) deployments are out of scope")

    def test_passphrase_must_match_network(self):
        self.manifest["network"]["passphrase"] = vdm.NETWORK_PASSPHRASES["futurenet"]
        self.assert_error("network.passphrase: does not match the testnet passphrase")

    def test_unsupported_version_is_rejected(self):
        for version in (2, "1", True):
            with self.subTest(version=version):
                self.manifest["manifest_version"] = version
                self.assert_error("manifest.manifest_version:")

    def test_unknown_field_is_rejected(self):
        self.manifest["contract"]["owner"] = DEPLOYER
        self.assert_error("contract: unknown field 'owner'")

    def test_contract_id_typo_fails_checksum(self):
        self.manifest["contract"]["contract_id"] = flip_last_char(CONTRACT_ID)
        self.assert_error("contract.contract_id: has an invalid checksum")

    def test_account_in_contract_field_is_rejected(self):
        self.manifest["contract"]["contract_id"] = DEPLOYER
        self.assert_error("contract.contract_id: must be a contract StrKey")

    def test_wasm_hash_must_be_lowercase_hex(self):
        self.manifest["contract"]["wasm_hash"] = WASM_HASH.upper()
        self.assert_error("contract.wasm_hash:")

    def test_short_commit_is_rejected(self):
        self.manifest["source"]["commit"] = "91ba5df"
        self.assert_error("source.commit:")

    def test_source_path_cannot_escape_repository(self):
        self.manifest["source"]["path"] = "../elsewhere"
        self.assert_error("source.path:")

    def test_timestamp_must_be_utc(self):
        self.manifest["deployment"]["deployed_at"] = "2026-10-09T11:12:17+01:00"
        self.assert_error("deployment.deployed_at:")

    def test_ledger_rejects_booleans(self):
        self.manifest["deployment"]["ledger"] = True
        self.assert_error("deployment.ledger:")

    def test_upload_transaction_may_be_null(self):
        self.manifest["deployment"]["upload_transaction"] = None
        self.assertEqual(offline_errors(self.manifest), [])

    def test_last_verified_cannot_predate_deployment(self):
        self.manifest["last_verified"] = {"date": "2026-10-01", "ledger": 1}
        errors = offline_errors(self.manifest)
        self.assertIn("last_verified.date: is earlier than deployment.deployed_at", errors)
        self.assertIn("last_verified.ledger: is earlier than deployment.ledger", errors)

    def test_duplicate_keys_are_rejected(self):
        with self.assertRaisesRegex(vdm.ManifestLoadError, "duplicate field 'contract_id'"):
            vdm.load_manifest('{"contract_id": "a", "contract_id": "b"}')

    def test_non_object_manifest_is_rejected(self):
        self.assertEqual(offline_errors([]), ["manifest: must be an object"])


class InitializationTests(unittest.TestCase):
    def setUp(self):
        self.manifest = load_testnet_manifest()

    def test_uninitialized_must_not_name_an_admin(self):
        self.manifest["initialization"]["admin"] = DEPLOYER
        self.assertIn("initialization.admin: must be null while the contract is uninitialized",
                      offline_errors(self.manifest))

    def test_initialized_requires_admin_token_and_transaction(self):
        self.manifest["initialization"]["status"] = "initialized"
        errors = offline_errors(self.manifest)
        self.assertTrue(any(e.startswith("initialization.admin:") for e in errors))
        self.assertIn("initialization.token: must be an object", errors)
        self.assertTrue(any(e.startswith("initialization.transaction:") for e in errors))

    def test_issued_asset_contract_passes(self):
        self.assertEqual(offline_errors(initialized(self.manifest, usdc_token())), [])

    def test_native_asset_contract_passes(self):
        token = usdc_token(contract_id=TESTNET_XLM_SAC, code="XLM", issuer=None)
        self.assertEqual(offline_errors(initialized(self.manifest, token)), [])

    def test_wrong_asset_contract_id_is_caught_offline(self):
        token = usdc_token(contract_id=TESTNET_XLM_SAC)
        errors = offline_errors(initialized(self.manifest, token))
        self.assertTrue(any("is not the Stellar Asset Contract for USDC" in e and TESTNET_USDC_SAC in e
                            for e in errors), errors)

    def test_issuer_required_for_non_native_asset(self):
        token = usdc_token(issuer=None)
        errors = offline_errors(initialized(self.manifest, token))
        self.assertTrue(any(e.startswith("initialization.token.issuer: is required") for e in errors))

    def test_asset_contract_decimals_are_fixed(self):
        errors = offline_errors(initialized(self.manifest, usdc_token(decimals=6)))
        self.assertIn("initialization.token.decimals: Stellar Asset Contracts always use 7 decimals", errors)

    def test_custom_token_contract_is_not_derived(self):
        token = usdc_token(kind="contract-token", contract_id=CONTRACT_ID, decimals=6)
        self.assertEqual(offline_errors(initialized(self.manifest, token)), [])


class XdrTests(unittest.TestCase):
    """Parsers checked against recorded public Testnet data."""

    def test_strkey_round_trip(self):
        payload = vdm.decode_strkey(CONTRACT_ID, "contract")
        self.assertEqual(vdm.encode_strkey(payload, "contract"), CONTRACT_ID)

    def test_instance_key_matches_rpc(self):
        key = vdm.instance_ledger_key(vdm.decode_strkey(CONTRACT_ID, "contract"))
        self.assertEqual(base64.b64encode(key).decode(), RECORDED["instance_entry"]["key"])

    def test_parse_recorded_instance(self):
        contract_hash = vdm.decode_strkey(CONTRACT_ID, "contract")
        data = base64.b64decode(RECORDED["instance_entry"]["xdr"])
        wasm_hash, storage = vdm.parse_instance_entry(data, contract_hash)
        self.assertEqual(wasm_hash.hex(), WASM_HASH)
        self.assertIsNone(storage)

    def test_instance_of_another_contract_is_rejected(self):
        data = base64.b64decode(RECORDED["instance_entry"]["xdr"])
        with self.assertRaises(ValueError):
            vdm.parse_instance_entry(data, bytes(32))

    def test_recorded_deploy_derives_contract_id(self):
        envelope = base64.b64decode(RECORDED["deploy_transaction"]["envelope_xdr"])
        deployer = vdm.decode_strkey(DEPLOYER, "account")
        [(salt, wasm_hash)] = vdm.find_create_contract(envelope, deployer)
        self.assertEqual(wasm_hash.hex(), WASM_HASH)
        self.assertEqual(vdm.deployed_contract_id(TESTNET, deployer, salt), CONTRACT_ID)

    def test_deploy_by_another_account_is_not_matched(self):
        envelope = base64.b64decode(RECORDED["deploy_transaction"]["envelope_xdr"])
        other = vdm.decode_strkey(TESTNET_USDC_ISSUER, "account")
        self.assertEqual(vdm.find_create_contract(envelope, other), [])

    def test_known_asset_contract_ids(self):
        usdc_issuer = vdm.decode_strkey(TESTNET_USDC_ISSUER, "account")
        self.assertEqual(vdm.asset_contract_id(TESTNET, "XLM", None), TESTNET_XLM_SAC)
        self.assertEqual(vdm.asset_contract_id(TESTNET, "USDC", usdc_issuer), TESTNET_USDC_SAC)

    def test_storage_key_padding(self):
        self.assertEqual(vdm.storage_key("Admin")[-8:], b"Admin\0\0\0")
        self.assertEqual(vdm.storage_key("Token")[-8:], b"Token\0\0\0")


class FakeNetwork:
    """In-memory stand-in for PublicNetworkClient."""

    def __init__(self, passphrase=TESTNET, latest_ledger=2_000):
        self.passphrase = passphrase
        self.latest_ledger = latest_ledger
        self.transactions = {}
        self.entries = {}

    def network_passphrase(self):
        return self.passphrase

    def transaction(self, tx_hash):
        return self.transactions.get(tx_hash)

    def ledger_entry(self, key):
        return self.entries.get(key), self.latest_ledger


class SyntheticDeployment:
    """Builds a self-consistent deployment, its manifest, and network responses."""

    DEPLOY_TX = "11" * 32
    UPLOAD_TX = "22" * 32
    INIT_TX = "33" * 32

    def __init__(self):
        self.deployer_key = bytes(range(32))
        self.deployer = vdm.encode_strkey(self.deployer_key, "account")
        self.salt = hashlib.sha256(b"salt").digest()
        self.wasm = b"\0asm\x01\0\0\0" + b"kolo-test-module"
        self.wasm_hash = hashlib.sha256(self.wasm).digest()
        self.contract_id = vdm.deployed_contract_id(TESTNET, self.deployer_key, self.salt)
        self.contract_hash = vdm.decode_strkey(self.contract_id, "contract")
        self.network = FakeNetwork()

        envelope = (b"\0" * 40 + u32(vdm.OP_INVOKE_HOST_FUNCTION) + u32(vdm.HOST_FN_CREATE_CONTRACT)
                    + u32(vdm.PREIMAGE_FROM_ADDRESS) + vdm.account_address(self.deployer_key) + self.salt
                    + u32(vdm.EXECUTABLE_WASM) + self.wasm_hash + b"\0" * 16)
        upload = (b"\0" * 40 + u32(vdm.OP_INVOKE_HOST_FUNCTION) + u32(vdm.HOST_FN_UPLOAD_WASM)
                  + u32(len(self.wasm)) + self.wasm + b"\0" * 16)
        self.add_transaction(self.DEPLOY_TX, envelope, ledger=1_500, created_at="2026-10-09T10:12:17Z")
        self.add_transaction(self.UPLOAD_TX, upload, ledger=1_499, created_at="2026-10-09T10:12:07Z")

        code_entry = (u32(vdm.LEDGER_ENTRY_CONTRACT_CODE) + u32(1) + b"\0" * 48 + self.wasm_hash
                      + u32(len(self.wasm)) + self.wasm)
        self.network.entries[vdm.code_ledger_key(self.wasm_hash)] = self.entry(code_entry)
        self.set_storage(None)

        self.manifest = load_testnet_manifest()
        self.manifest["contract"].update(contract_id=self.contract_id, wasm_hash=self.wasm_hash.hex(),
                                         wasm_size_bytes=len(self.wasm))
        self.manifest["deployment"].update(deployer=self.deployer, ledger=1_500,
                                           deployed_at="2026-10-09T10:12:17Z",
                                           deploy_transaction=self.DEPLOY_TX,
                                           upload_transaction=self.UPLOAD_TX)
        self.manifest["last_verified"]["ledger"] = 1_900

    def entry(self, data, live_until=None):
        live = self.network.latest_ledger + 100 if live_until is None else live_until
        return {"xdr": base64.b64encode(data).decode(), "liveUntilLedgerSeq": live}

    def add_transaction(self, tx_hash, envelope, ledger, created_at, source=None, successful=True):
        self.network.transactions[tx_hash] = {
            "hash": tx_hash,
            "successful": successful,
            "ledger": ledger,
            "created_at": created_at,
            "source_account": source or self.deployer,
            "envelope_xdr": base64.b64encode(envelope).decode(),
        }

    def set_storage(self, entries, live_until=None):
        storage = u32(0)
        if entries is not None:
            storage = u32(1) + u32(len(entries)) + b"".join(entries)
        instance = (u32(vdm.LEDGER_ENTRY_CONTRACT_DATA) + u32(0) + vdm.contract_address(self.contract_hash)
                    + u32(vdm.SCV_LEDGER_KEY_CONTRACT_INSTANCE) + u32(vdm.DURABILITY_PERSISTENT)
                    + u32(vdm.SCV_CONTRACT_INSTANCE) + u32(vdm.EXECUTABLE_WASM) + self.wasm_hash + storage)
        self.network.entries[vdm.instance_ledger_key(self.contract_hash)] = self.entry(instance, live_until)

    def initialize(self, admin_key, token_hash):
        self.set_storage([
            vdm.storage_key("Admin") + u32(vdm.SCV_ADDRESS) + vdm.account_address(admin_key),
            vdm.storage_key("Token") + u32(vdm.SCV_ADDRESS) + vdm.contract_address(token_hash),
        ])
        self.add_transaction(self.INIT_TX, b"", ledger=1_600, created_at="2026-10-10T00:00:00Z")

    def run(self):
        report = vdm.Report()
        vdm.check_offline(self.manifest, report)
        self.offline_errors = list(report.errors)
        vdm.check_online(self.manifest, report, self.network)
        return report


class OnlineVerificationTests(unittest.TestCase):
    def setUp(self):
        self.deployment = SyntheticDeployment()

    def run_checks(self):
        report = self.deployment.run()
        self.assertEqual(self.deployment.offline_errors, [], "fixture should pass offline checks")
        return report

    def test_consistent_deployment_passes(self):
        report = self.run_checks()
        self.assertEqual(report.errors, [])
        self.assertEqual(report.warnings, [])
        self.assertTrue(any("derives contract ID" in line for line in report.verified))

    def test_wrong_network_stops_verification(self):
        self.deployment.network.passphrase = vdm.MAINNET_PASSPHRASE
        report = self.run_checks()
        self.assertEqual(len(report.errors), 1)
        self.assertTrue(report.errors[0].startswith("network.rpc_url: reports passphrase"))

    def test_contract_not_created_by_deploy_transaction(self):
        other = vdm.deployed_contract_id(TESTNET, self.deployment.deployer_key, bytes(32))
        self.deployment.manifest["contract"]["contract_id"] = other
        errors = self.run_checks().errors
        self.assertTrue(any(e.startswith("deployment.deploy_transaction: creates") for e in errors))
        self.assertIn("contract.contract_id: no contract instance exists on this network", errors)

    def test_deploy_sent_by_another_account(self):
        tx = self.deployment.network.transactions[SyntheticDeployment.DEPLOY_TX]
        tx["source_account"] = TESTNET_USDC_ISSUER
        errors = self.run_checks().errors
        self.assertTrue(any("not the deployer" in e for e in errors))

    def test_failed_deploy_transaction(self):
        self.deployment.network.transactions[SyntheticDeployment.DEPLOY_TX]["successful"] = False
        self.assertTrue(any("did not succeed" in e for e in self.run_checks().errors))

    def test_wrong_ledger_and_time(self):
        self.deployment.manifest["deployment"].update(ledger=1_400, deployed_at="2026-10-09T10:00:00Z")
        errors = self.run_checks().errors
        self.assertIn("deployment.ledger: deploy transaction is in ledger 1500", errors)
        self.assertIn("deployment.deployed_at: deploy transaction closed at 2026-10-09T10:12:17Z", errors)

    def test_missing_upload_transaction(self):
        self.deployment.manifest["deployment"]["upload_transaction"] = "44" * 32
        self.assertTrue(any("was not found" in e for e in self.run_checks().errors))

    def test_wrong_wasm_size(self):
        self.deployment.manifest["contract"]["wasm_size_bytes"] = 1
        self.assertIn(f"contract.wasm_size_bytes: stored Wasm is {len(self.deployment.wasm)} bytes",
                      self.run_checks().errors)

    def test_expired_entries_warn_but_pass(self):
        self.deployment.set_storage(None, live_until=0)
        report = self.run_checks()
        self.assertEqual(report.errors, [])
        self.assertTrue(any("contract instance is not live" in w for w in report.warnings))

    def test_manifest_claims_uninitialized_but_chain_disagrees(self):
        self.deployment.initialize(bytes(range(32)), vdm.decode_strkey(TESTNET_USDC_SAC, "contract"))
        self.assertIn("initialization.status: the contract has been initialized on-chain",
                      self.run_checks().errors)

    def test_initialized_admin_and_token_match(self):
        admin = vdm.encode_strkey(bytes(range(1, 33)), "account")
        self.deployment.initialize(bytes(range(1, 33)), vdm.decode_strkey(TESTNET_USDC_SAC, "contract"))
        initialized(self.deployment.manifest, usdc_token(), admin=admin)
        self.deployment.manifest["initialization"]["transaction"] = SyntheticDeployment.INIT_TX
        report = self.run_checks()
        self.assertEqual(report.errors, [])
        self.assertTrue(any(f"stores admin {admin}" in line for line in report.verified))

    def test_initialized_with_different_admin_and_token(self):
        self.deployment.initialize(bytes(range(1, 33)), vdm.decode_strkey(TESTNET_XLM_SAC, "contract"))
        initialized(self.deployment.manifest, usdc_token(), admin=self.deployment.deployer)
        self.deployment.manifest["initialization"]["transaction"] = SyntheticDeployment.INIT_TX
        errors = self.run_checks().errors
        self.assertIn("initialization.admin: does not match the admin stored in the contract instance", errors)
        self.assertIn("initialization.token.contract_id: does not match the token stored in the "
                      "contract instance", errors)

    def test_manifest_claims_initialized_but_chain_is_empty(self):
        initialized(self.deployment.manifest, usdc_token())
        self.assertIn("initialization.status: the contract instance has no admin; it is not initialized",
                      self.run_checks().errors)


class CommandLineTests(unittest.TestCase):
    def run_main(self, *args):
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            code = vdm.main([str(arg) for arg in args])
        return code, buffer.getvalue()

    def test_valid_manifest_exits_zero(self):
        code, output = self.run_main(TESTNET_MANIFEST)
        self.assertEqual(code, 0)
        self.assertIn("PASS", output)

    def test_invalid_manifest_exits_one(self):
        manifest = load_testnet_manifest()
        broken = copy.deepcopy(manifest)
        broken["production"] = True
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "broken.json"
            path.write_text(json.dumps(broken), encoding="utf-8")
            code, output = self.run_main(path)
        self.assertEqual(code, 1)
        self.assertIn("FAIL", output)
        self.assertIn("manifest.production", output)


if __name__ == "__main__":
    unittest.main()
