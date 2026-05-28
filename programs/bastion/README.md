# Bastion

Programmable policy runtime for Solana session keys.

The Bastion on-chain program lets wallets delegate constrained authority to
session keys through composable policy accounts enforced at runtime.
---

# High-level Architecture

```mermaid
flowchart LR
    Owner[Wallet Owner]
    SessionKey[Session Key]

    Session["Session PDA\nseeds:\n['session', owner, session_key]"]

    Delegate["Delegate PDA\nseeds:\n['delegate', owner, session_key]"]

    Policy1["Policy PDA #0"]
    Policy2["Policy PDA #1"]
    PolicyN["Policy PDA #N"]

    Owner --> Session
    Owner --> Delegate

    Session --> Policy1
    Session --> Policy2
    Session --> PolicyN

    SessionKey -->|signs execute| Session
    Delegate -->|invoke_signed signer| CPI["Wrapped CPI"]
```

---

# Account Model

```mermaid
classDiagram
    class Session {
        Pubkey owner
        Pubkey session_key
        bool revoked
        i64 expires_at
        [32] policies_hash
        u8 policy_count
    }

    class Policy {
        Pubkey session
        u8 seed
        PolicyKind kind
        bytes data
    }

    class Delegate {
        lamports
        SPL approvals
    }

    Session --> Policy
    Session --> Delegate
```

---

# Execute Flow

`execute(wrapped_ix)` is the hot path.

```mermaid
sequenceDiagram
    participant SK as Session Key
    participant B as Bastion
    participant P as Policies
    participant D as Delegate PDA
    participant TP as Target Program

    SK->>B: execute(wrapped_ix)

    B->>B: Validate session
    B->>B: Check revoked / expiry
    B->>B: Verify attached policy set hash

    loop For each policy
        B->>P: validate()
    end

    B->>P: Pre-charge RateLimit
    B->>P: Snapshot SpendCap

    B->>TP: invoke_signed(delegate PDA)

    TP-->>B: CPI result

    B->>P: Post-charge SpendCap
```

---

# PDA Layout

```mermaid
flowchart TD
    Session["Session PDA"]

    subgraph Policies[Policy PDAs]
        P0["policy #0"]
        P1["policy #1"]
        P2["..."]
    end

    Delegate["Delegate PDA"]

    Session --> P0
    Session --> P1
    Session --> P2

    Session --> Delegate
```

---

# Instructions

| Instruction      | Signer      | Purpose                                           |
| ---------------- | ----------- | ------------------------------------------------- |
| `init_session`   | owner       | Create a Session PDA                              |
| `attach_policy`  | owner       | Attach a new Policy PDA and re-hash session state |
| `update_policy`  | owner       | Replace policy data (kind-preserving realloc)     |
| `detach_policy`  | owner       | Remove Policy PDA and re-hash session             |
| `revoke_session` | owner       | Permanently revoke a session                      |
| `close_session`  | owner       | Close Session + child Policies                    |
| `sweep_delegate` | owner       | Drain Delegate PDA lamports                       |
| `execute`        | session_key | Execute wrapped CPI through policy runtime        |

---

# Policy Runtime

```mermaid
flowchart LR
    WrappedIx[Wrapped Instruction]
    Runtime[Bastion Runtime]

    ProgramAllow["Program Allowlist"]
    ProgramBlock["Program Blocklist"]

    MintAllow["Mint Allowlist"]
    MintBlock["Mint Blocklist"]

    NFTAllow["NFT Collection Allowlist"]
    NFTBlock["NFT Collection Blocklist"]

    RateLimit["Rate Limit"]
    SpendCap["Spend Cap"]
    Expiry["Expiry"]
    ForeignSigner["Foreign Signer Gate"]

    WrappedIx --> Runtime

    Runtime --> ProgramAllow
    Runtime --> ProgramBlock
    Runtime --> MintAllow
    Runtime --> MintBlock
    Runtime --> NFTAllow
    Runtime --> NFTBlock
    Runtime --> RateLimit
    Runtime --> SpendCap
    Runtime --> Expiry
    Runtime --> ForeignSigner
```

---

# Policy Kinds

* `ProgramAllowlist`
* `ProgramBlocklist`
* `MintAllowlist`
* `MintBlocklist`
* `NftCollectionAllowlist`
* `NftCollectionBlocklist`
* `RateLimit`
* `SpendCap`

  * `NativeSol`
  * `SplToken`
  * `Token2022`
* `Expiry`
* `ForeignSignerNotAllowed`

---

# Development

```bash
# Build the project
anchor build

# Run all tests
anchor run testsvm

# Run a single integration suite
cargo test -p bastion --test test_demo_scenario
```

LiteSVM integration tests embed the generated `.so` directly via
`include_bytes!`, so the SBF build must be rebuilt before running tests.

---

# Test Architecture

```mermaid
flowchart TD
    Harness["tests/common/mod.rs\nLiteSVM harness\nPDA helpers\ntoken/NFT fixtures"]

    Init["test_init_session.rs"]
    Revoke["test_revoke_session.rs"]
    Close["test_close_session.rs"]

    Attach["test_attach_policy.rs"]
    Detach["test_detach_policy.rs"]
    Update["test_update_policy.rs"]

    Execute["test_execute_skeleton.rs"]
    ExecutePolicies["test_execute_policies.rs"]

    Program["test_program_lists.rs"]
    Mint["test_mint_lists.rs"]
    NFT["test_nft_collection.rs"]

    Rate["test_rate_limit.rs"]
    Spend["test_spend_cap.rs"]

    Demo["test_demo_scenario.rs"]

    Harness --> Init
    Harness --> Revoke
    Harness --> Close
    Harness --> Attach
    Harness --> Detach
    Harness --> Update
    Harness --> Execute
    Harness --> ExecutePolicies
    Harness --> Program
    Harness --> Mint
    Harness --> NFT
    Harness --> Rate
    Harness --> Spend
    Harness --> Demo
```