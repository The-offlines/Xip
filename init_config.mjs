import * as anchor from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import fs from "fs";

const connection = new Connection("https://api.devnet.solana.com", "confirmed");
const wallet = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync(process.env.HOME + "/.config/solana/id.json")))
);
const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(wallet), {});
anchor.setProvider(provider);

const idl = JSON.parse(fs.readFileSync("./target/idl/xip.json"));
const programId = new PublicKey("GEmKyTKdMMJEi31c592eee9GDgxDMbeWNUYWNGcCoh3U");
const program = new anchor.Program(idl, provider);

// Devnet USDC mint
const usdcMint = new PublicKey("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");

// Use wallet as attester for now (devnet only)
const solflareAttester = wallet.publicKey;

const [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
const treasury = new PublicKey("FHoV1GUTsUTPrFi2D5j8iEBe5tshSCGsQvLZJWf1F9nV");

console.log("Config PDA:", configPda.toString());
console.log("Calling initialize_config...");

const tx = await program.methods
  .initializeConfig(usdcMint, solflareAttester)
  .accounts({
    authority: wallet.publicKey,
    config: configPda,
    treasury: treasury,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .signers([wallet])
  .rpc();

console.log("✅ Success! TX:", tx);
