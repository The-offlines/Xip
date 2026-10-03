# XiP Fee Quotes

The protocol settles fees in the same asset as the tip. The web UI can additionally show a USD/USDC value and an equivalent SOL amount.

## Settlement

For a requested tip amount `T`:

- Standard wallet: `fee = floor(T * 100 / 10_000)` = 1%
- Verified Solflare path: `fee = floor(T * 50 / 10_000)` = 0.5%
- Recipient receives exactly `T`
- Sender pays `T + fee`

## Display quotes

USDC tips:

```text
fee_usdc = fee_base_units / 1_000_000
fee_sol  = fee_usdc / SOL_USD
```

SOL tips:

```text
fee_sol  = fee_lamports / 1_000_000_000
fee_usdc = fee_sol * SOL_USD
```

For a UI-only estimate, `SOL_USD` should come from a trusted backend price service/oracle and be timestamped. Never let a client-supplied price determine the amount transferred on-chain.

For protocol logic that actually depends on USD value, the price must instead be verified on-chain with a trusted oracle and freshness/confidence bounds. Pyth currently documents both pull and push integration patterns on Solana. citeturn355748search0turn355748search7

## Examples

Assume SOL = $200.

A `$10 USDC` tip using a standard wallet:

```text
Tip:        $10.00 USDC
XiP fee:     $0.10 USDC
Fee ≈       0.0005 SOL
Sender:     $10.10 USDC
Creator:    $10.00 USDC
```

A `$10 USDC` tip using the verified Solflare discount:

```text
Tip:        $10.00 USDC
XiP fee:     $0.05 USDC
Fee ≈       0.00025 SOL
Sender:     $10.05 USDC
Creator:    $10.00 USDC
```
