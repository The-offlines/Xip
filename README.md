# XiP Anchor Program

XiP's on-chain settlement layer for Solana-native X tipping.

## Protocol rules

- Solana only.
- Allowed payment assets: SOL and USDC on Solana.
- Standard sender fee: 1.00%.
- Solflare-authorized sender fee: 0.50%.
- Recipient fee: 0%.
- Treasury: `FHoV1GUTsUTPrFi2D5j8iEBe5tshSCGsQvLZJWf1F9nV`.
- Unclaimed recipients are supported for SOL and USDC.
- Anonymous means XiP-layer attribution is hidden. The blockchain still exposes the signing wallet.

## USDC safety

The program only permits the exact official Solana USDC mints below:

- Mainnet: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
- Devnet: `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU`

`Config.usdc_mint` is fixed when `initialize_config` is executed. Do not use arbitrary SPL mints.

Solana's current production-readiness guidance recommends allowlisting token mint addresses and keeping Devnet/Mainnet token configuration separate. citeturn967619search0

## Fee calculation

Settlement is always in the tipped asset:

- SOL tip -> fee in lamports.
- USDC tip -> fee in USDC base units.

The on-chain fee calculator uses checked `u128` multiplication and rounds upward so a positive tip never silently produces a zero fee.

The UI can display the equivalent value in the other asset using a trusted SOL/USD quote. That quote must be informational only and must never control the settlement amount.

Example:

```text
SOL tip:
1.00 SOL
XiP fee: 0.010 SOL
USDC equivalent: shown by UI using trusted price quote
```

```text
USDC tip:
10.00 USDC
XiP fee: 0.10 USDC
SOL equivalent: shown by UI using trusted price quote
```

## Solflare discount

A Solana transaction cannot prove which wallet UI produced a signature. Therefore the 0.50% path requires the configured `solflare_attester` to sign the transaction after XiP independently verifies the sender's Solflare flow.

The attester is intentionally separate from the protocol upgrade authority. It should be held in an isolated signer/KMS or equivalent secure environment.

Compromise of the attester should only be able to reduce XiP fee revenue by authorizing the discounted tier; it must not be able to redirect tips or treasury funds.

## Identity model

X OAuth is off-chain. The program stores a 32-byte `x_user_hash` and a verified Solana wallet. The backend authority attests the X identity while the claimant wallet signs the claim transaction.

X usernames are not used as canonical identifiers because usernames can change.

## Unclaimed tips

A recipient does not need to register with XiP before receiving a tip.

For an unclaimed identity, the nominal tip is held in:

- a program-owned SOL PDA, or
- a program-owned SPL-token vault for USDC.

Later, the XiP authority attests the X identity and the claimant wallet signs the claim. The claimant receives the full tip amount. There is no recipient fee.

## Security invariants

- Treasury is hard-coded and checked on every settlement.
- USDC is allowlisted by exact mint address.
- Claimed recipient wallet is constrained by the Profile PDA.
- Unclaimed vaults are PDA-derived from X identity + configured USDC mint.
- SPL settlement uses `transfer_checked`.
- Fee math uses checked arithmetic.
- Solflare discount requires a dedicated authority signature.
- Wallet rotation requires both old and new wallet signatures.
- No seed phrase, private key, or arbitrary token mint handling exists in the program.

## Build

The project currently targets Anchor 1.1.2.

```bash
anchor build
```

Current Anchor documentation recommends modular project structure and `anchor build` for compilation. citeturn597865search4turn597865search6

For reproducible production artifacts:

```bash
anchor build --verifiable
```

Anchor documents verifiable builds using pinned Docker images. citeturn597865search7

## Devnet setup

Set your Solana CLI to Devnet and make sure the deployer wallet exists before deployment:

```bash
solana config set --url https://api.devnet.solana.com
solana address
solana balance
```

Devnet is Solana's public testing environment and uses non-real tokens. citeturn967619search5

The next setup step is to initialize the config account with the Devnet USDC mint and a dedicated Solflare attester public key, then run the complete localnet/devnet integration suite before any mainnet deployment.

## Production gate

Do not deploy to mainnet until:

1. Full success/failure tests pass.
2. Devnet end-to-end testing passes.
3. Upgrade authority is secured behind multisig/governance.
4. Solflare attester is isolated from the upgrade key.
5. Claim and unclaimed-balance flows are independently reviewed.
6. Fee and token-mint invariants are independently audited.
7. A verifiable build is produced and retained.
8. The program has an independent security audit.
