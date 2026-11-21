# Deployment manifests

A deployment manifest is a small JSON file that records the public facts of one contract deployment: the network, contract ID, Wasm hash, deployer, source commit, and the admin and token chosen when the group is initialized. A reviewer should be able to check every value against the network and the repository, without trusting the manifest or whoever wrote it.

Manifests are for **non-production deployments only**. Every version 1 manifest sets `"production": false` and carries a human-readable `notice`. The validator rejects Mainnet manifests. Production deployments are out of scope.

```text
deployments/
├── manifest.v1.schema.json        format definition (JSON Schema, draft 2020-12)
└── testnet/
    └── kolo-savings-group.json    the current Testnet deployment
```

## Current Testnet deployment

[`testnet/kolo-savings-group.json`](testnet/kolo-savings-group.json) describes contract [`CBQCB6KIXAPDPKGNZFR3KSBBJNEJRFN3MAQL7S46JGFA5I22AUL4OPYL`](https://stellar.expert/explorer/testnet/contract/CBQCB6KIXAPDPKGNZFR3KSBBJNEJRFN3MAQL7S46JGFA5I22AUL4OPYL). Things a reviewer should know:

- **It is uninitialized.** No admin or token has been set, so `initialization.admin` and `initialization.token` are `null`.
- **It runs older code.** The deployed Wasm was built from commit `91ba5df`. Contract source has changed on `main` since then (for example, partial payouts), so the deployment does not reflect the current interface.
- **Its ledger entries have expired.** As of ledger 5849280, the contract instance and code entries were no longer live. Anyone invoking the contract must restore them first (`stellar contract restore`). The validator reports this as a warning.

## Format (version 1)

All values are public. Each comes from an on-chain record or from the public repository.

| Field | Meaning | Where to verify it |
| --- | --- | --- |
| `manifest_version` | Format version. Always `1` for this schema. | — |
| `production` | Always `false`. Marks the deployment as non-production. | — |
| `notice` | Warning for anyone reading the file. | — |
| `network.name`, `network.passphrase` | `testnet` or `futurenet` and its exact passphrase. | RPC `getNetwork` |
| `network.rpc_url`, `network.horizon_url`, `network.explorer_url` | Public endpoints used for verification. Must be `https://` with no embedded credentials. | — |
| `contract.contract_id` | Contract address (`C…`). | Re-derived from the deploy transaction |
| `contract.wasm_hash` | SHA-256 of the uploaded Wasm. Soroban uses this value as the code's identifier. | Contract instance and code entries |
| `contract.wasm_size_bytes` | Size of the uploaded Wasm (optional). | Contract code entry |
| `source.repository`, `source.commit`, `source.path` | Where the code came from (full 40-character SHA). | The repository |
| `source.provenance` | How strongly the Wasm is tied to the commit; see below. | Rebuild or interface comparison |
| `source.toolchain` | rustc, Soroban SDK, target, and optimizer used (optional). | `stellar contract info meta` |
| `deployment.deployer` | **Public** account (`G…`) that sent the transactions. | Horizon transaction source |
| `deployment.deployed_at`, `deployment.ledger` | Close time and ledger of the deploy transaction. | Horizon |
| `deployment.upload_transaction` | Transaction that uploaded the Wasm, or `null` if the Wasm was already on-chain. | Horizon envelope |
| `deployment.deploy_transaction` | Transaction that created the contract. | Horizon envelope |
| `initialization.status` | `uninitialized` or `initialized`. | Contract instance storage |
| `initialization.admin` | Admin account passed to `initialize`, or `null`. | Contract instance storage |
| `initialization.token` | Token configured at `initialize`, or `null`; see below. | Contract instance storage |
| `initialization.transaction` | The `initialize` transaction, or `null`. | Horizon |
| `last_verified` | Date and ledger of the last successful `--online` check (optional). | — |

`source.provenance` takes one of three values:

- `reproducible-build`: rebuilding `source.commit` with the recorded toolchain produces bytes whose SHA-256 is `contract.wasm_hash`.
- `interface-match`: the deployed contract spec (the `contractspecv0` section that defines the public interface) matches a build of the commit, but the exact bytes were not reproduced. The current Testnet deployment is at this level, because the optimizer version used was not recorded.
- `unverified`: no check has been made.

`initialization.token` describes the asset with `kind`, `contract_id`, `code`, `issuer`, and `decimals`. For `kind: "stellar-asset-contract"`, the contract ID is derived from the asset code, issuer, and network, so the validator checks it without any network lookup. A wrong USDC address fails validation offline. An `issuer` is required for every asset except native XLM, because an asset code alone does not identify a Stellar asset.

### What never goes in a manifest

Secret keys (`S…`), seed phrases, private key files, signing identities, `.env` contents, and URLs with embedded credentials. The validator scans the raw file for secret-seed patterns and private-key blocks, even if the JSON is malformed. It also rejects field names such as `secret`, `private`, `seed`, `mnemonic`, or `signing`, and any field the format does not define. Error messages name the location but never repeat the matched value. If a key ever reaches a manifest, treat it as compromised and rotate it. Deleting the line is not enough.

## Validating a manifest

The validator is [`scripts/validate_deployment_manifest.py`](../scripts/validate_deployment_manifest.py). It needs only Python 3.9+ and no third-party packages. Run it from the repository root:

```bash
# Structure, formats, StrKey checksums, network passphrase, Stellar Asset
# Contract IDs, non-production marking, and the secret scan. No network access.
python3 scripts/validate_deployment_manifest.py deployments/testnet/*.json

# Also re-derive every on-chain claim from the manifest's public endpoints.
python3 scripts/validate_deployment_manifest.py --online deployments/testnet/kolo-savings-group.json
```

`--online` makes read-only requests to the manifest's Horizon and Soroban RPC URLs and needs no keys. It checks that:

1. The RPC endpoint reports the manifest's network passphrase. If it does not, verification stops, because every later answer would come from the wrong network.
2. The deploy transaction succeeded, was sent by `deployment.deployer`, and closed at the recorded ledger and time.
3. Hashing the deployer address, the salt in that transaction, and the network ID produces exactly `contract.contract_id`, created with `contract.wasm_hash`.
4. The upload transaction succeeded, was sent by the deployer, and carries Wasm whose SHA-256 is `contract.wasm_hash`.
5. The live contract instance runs `contract.wasm_hash`, and its storage agrees with `initialization`: no admin while uninitialized, or exactly the recorded admin and token once initialized.
6. The stored Wasm bytes hash to `contract.wasm_hash` and match `contract.wasm_size_bytes`.

Expired ledger entries produce a warning, not a failure.

Exit codes: `0` valid, `1` invalid, `2` usage error, `3` a public endpoint could not be reached, so online verification is incomplete.

CI runs the offline validation and the validator's tests on every push and pull request. The online check runs on request, so CI does not depend on Testnet availability.

The validator's tests are in [`scripts/test_validate_deployment_manifest.py`](../scripts/test_validate_deployment_manifest.py):

```bash
python3 -m unittest discover -s scripts -p "test_*.py"
```

## Verifying a deployment by hand with the Stellar CLI

You can verify a deployment without the validator, using only the Stellar CLI and public endpoints. The commands pass the RPC URL and passphrase explicitly so they do not depend on local CLI configuration:

```bash
RPC=https://soroban-testnet.stellar.org
PASSPHRASE="Test SDF Network ; September 2015"
CONTRACT=CBQCB6KIXAPDPKGNZFR3KSBBJNEJRFN3MAQL7S46JGFA5I22AUL4OPYL
```

1. **The Wasm hash.** Download the code the contract runs and hash it. The result must equal `contract.wasm_hash`.

   ```bash
   stellar contract fetch --id "$CONTRACT" --rpc-url "$RPC" --network-passphrase "$PASSPHRASE" -o deployed.wasm
   sha256sum deployed.wasm
   ```

2. **The source commit.** Build the recorded commit and compare its public interface with the deployed one. The two outputs should be identical.

   ```bash
   git checkout 91ba5dfe2c7267a202db5241f34b8d8a83908f05
   (cd contracts && cargo build --target wasm32v1-none --release)
   stellar contract info interface --wasm contracts/target/wasm32v1-none/release/kolo_savings_group.wasm --output json-formatted > built.json
   stellar contract info interface --contract-id "$CONTRACT" --rpc-url "$RPC" --network-passphrase "$PASSPHRASE" --output json-formatted > deployed.json
   diff built.json deployed.json && echo "interface matches"
   ```

   `stellar contract info meta --contract-id "$CONTRACT" …` shows the rustc and Soroban SDK versions embedded at build time. Compare them with `source.toolchain`.

3. **The deployer and transactions.** Open each transaction on Stellar Expert, or query Horizon directly. Check that it is `successful`, and that `source_account` and `ledger` match the manifest:

   ```bash
   curl -s https://horizon-testnet.stellar.org/transactions/1d539b2c8f345601a54e039779a46b4fa14ccfee0e553884c2531bc2caec6f74
   ```

If `stellar contract fetch` reports that an entry is archived, it must be restored before it can be read or invoked. Restoring sends a transaction from an account you control on that network, so use a throwaway Testnet identity.

## Recording a new deployment

1. Deploy to Testnet, from CI or with `contracts/deploy.sh`. Note the contract ID it prints, the commit you deployed, and the deployer's public address. The Wasm hash is the SHA-256 of the optimized Wasm you uploaded (`sha256sum`).
2. Find the upload and deploy transaction hashes in the deployer's recent operations on Stellar Expert, or at `https://horizon-testnet.stellar.org/accounts/<deployer>/operations?order=desc`.
3. Copy an existing manifest under `deployments/<network>/` and fill in the new values. Set `source.provenance` to the strongest level you actually checked.
4. Run the validator with `--online` until it passes. Then set `last_verified` to that date and ledger.
5. After the group is initialized, set `initialization` to `initialized` with the admin, token, and `initialize` transaction, and re-run `--online`.

Never paste a value from a secret store, `.env` file, or CLI identity file into a manifest. Everything a manifest needs is already visible on a block explorer.

## Versioning

`manifest_version` is an integer, and each version has its own schema file (`manifest.v1.schema.json`, and so on). Within a version, fields are only ever added as optional. Removing a field, renaming it, making it required, or changing its meaning requires a new version, a new schema file, and validator support. The validator rejects versions it does not know, so an old validator cannot silently accept a newer manifest.
