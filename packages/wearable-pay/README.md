# 👓 ApexGlass: Wearable & Smart Glasses Solana Pay System

[![Solana](https://img.shields.io/badge/Solana-Pay-14F195?style=for-the-badge&logo=solana&logoColor=white)](https://solana.com/pay)
[![Omi](https://img.shields.io/badge/BasedHardware-Omi_Wearable-black?style=for-the-badge)](https://github.com/BasedHardware/omi)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

**ApexGlass** is an open-source optical & voice settlement engine designed for AI smart glasses and wearables (Omi, Meta Ray-Ban, Apple Vision Pro, Android smartglasses). It bridges physical world merchant interactions with sub-second Solana SPL token settlements.

---

## 💡 How It Works

1. **Optical Capture (Smart Glasses Camera):**
   * As the user looks at a Solana Pay QR code in a store, restaurant, or terminal, the camera captures the Solana Pay URI (`solana:<merchant>?amount=4.50&spl-token=USDC&label=CoffeeShop`).
2. **Audio & Voice Interaction (Bone Conduction Speakers / Mic):**
   * The AI wearable whispers in the user's ear: *"Detected payment to Coffee Shop for 4.50 USDC. Say 'Approve' to confirm or 'Cancel' to reject."*
   * The user simply replies: *"Approve"*.
3. **Sub-400ms On-Chain Execution:**
   * The transaction signs and broadcasts directly to the Solana network in under 400 milliseconds.
   * A gentle chime plays in the glasses: *"Payment confirmed on Solana. Receipt logged."*

---

## 🛠️ Architecture & Components

```
packages/wearable-pay/
├── src/
│   ├── index.ts          # Core TypeScript SDK (URI parser, Solana Pay builder, settlement runner)
│   └── omi_plugin.py     # Python integration plugin for BasedHardware/omi companion app
├── package.json          # Dependencies: @solana/web3.js, @solana/pay, @solana/spl-token
└── README.md             # This document
```

---

## 🚀 Monetization & Value Creation

1. **Protocol Micro-Surcharge:**
   * 0.25% fee on all merchant transactions processed through the ApexYield treasury vault.
2. **BasedHardware/Omi App Store & Bounty:**
   * Published as an official plugin for Omi users.
   * Eligible for $100–$250 maintainer bounty review on `BasedHardware/omi`.
3. **Solana Foundation & Hackathon Grants:**
   * Milestone 3 deliverable for the $10,000 Solana Foundation Developer Tooling Grant.
