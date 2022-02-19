const { assert } = require('chai');
const anchor = require('@project-serum/anchor');
const common = require('@project-serum/common');
const nacl = require('tweetnacl');
nacl.util = require('tweetnacl-util');

const {
  TOKEN_PROGRAM_ID, 
  ASSOCIATED_TOKEN_PROGRAM_ID, 
  Token
} = require("@solana/spl-token");

const {BN} = anchor;
const { 
  SystemProgram, 
  Keypair,
  PublicKey,
  SYSVAR_RENT_PUBKEY,
  LAMPORTS_PER_SOL
} = anchor.web3;

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms))
}

async function createTokenAccount(provider, mint, owner) {
  const vault = anchor.web3.Keypair.generate();
  const tx = new anchor.web3.Transaction();
  tx.add(
    ...(await common.createTokenAccountInstrs(provider, vault.publicKey, mint, owner))
  );
  await provider.send(tx, [vault]);
  return vault.publicKey;
}

async function createAssociatedTokenAccount(provider, mint, associatedAccount, owner, payer, signer) {

  const tx = new anchor.web3.Transaction();
  tx.add(
    await Token.createAssociatedTokenAccountInstruction(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      mint,
      associatedAccount,
      owner,
      payer
    )
  );
  await provider.send(tx, [signer]);
  return associatedAccount;
}

async function mintTo(provider, mint, dest, authority, amount, signer) {
  const tx = new anchor.web3.Transaction();
  tx.add(
    await Token.createMintToInstruction(
      TOKEN_PROGRAM_ID,
      mint,
      dest,
      authority,
      [signer],
      amount
    )
  );
  await provider.send(tx, [signer]);
}

describe('spl-token-timelock', () => {

  // Configure the client to use the local cluster.
  const provider = anchor.Provider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SplTokenTimelock;
  const DECIMALS = 9;

  const start = new BN(+new Date() / 1000 + 4);
  const cliff = new BN(+new Date() / 1000 + 5);
  const end = new BN(+new Date() / 1000 + 60); 
  
  const period = new BN(1);
  
  const depositedAmount = new BN(10 * LAMPORTS_PER_SOL);

  let nonce;
  let mint;
  let granterToken;
  let granter = provider.wallet;
  let recipientToken;
  let escrowVault;
  const recipient = Keypair.generate();
  const vesting = Keypair.generate();

  before(async () => {

    mint = await common.createMint(
        provider,
        granter.publicKey,
        DECIMALS
    );
    
    granterToken = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      mint,
      granter.publicKey
    );
    
    await createAssociatedTokenAccount(
      provider,
      mint,
      granterToken,
      granter.publicKey,
      granter.publicKey,
      granter.payer
    );
    
    await mintTo(
      provider,
      mint,
      granterToken,
      granter.publicKey,
      10 * LAMPORTS_PER_SOL,
      granter.payer
    );
    
    [escrowVault, nonce] = await PublicKey.findProgramAddress(
      [vesting.publicKey.toBuffer()],
      program.programId
    );
    
    recipientToken = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      mint,
      recipient.publicKey
    );
        
    console.log("\nBefore:");
    console.log("programId",program.programId.toBase58());
    console.log("granter wallet:", granter.publicKey.toBase58());
    console.log("granter token:", granterToken.toBase58());
    console.log("escrowVault (vesting):", vesting.publicKey.toBase58());
    console.log("escrowVault token:", escrowVault.toBase58());
    console.log("recipient wallet:", recipient.publicKey.toBase58());
    console.log("recipient token:", recipientToken.toBase58());
    console.log("mint:", mint.toBase58());
    console.log("nonce:", nonce);
  });

  it("Create vesting", async() => {
      
      console.log("\nCreate vesting:");

      let listener = null;
      listener = program.addEventListener("CreateVestingEvent", (event, slot) => {
        console.log("slot: ", slot);
        console.log("event data: ",event.data.toNumber());
        console.log("event status: ",event.status);
      });

      let vesting_name = nacl.util.decodeUTF8("GoGo Corp");
      let investor_wallet_address = nacl.util.decodeUTF8("0x519d6DCdf1acbFD8774751F1043deeeA8778ef4a");
      const tx = await program.rpc.createVesting(
        depositedAmount,
        nonce,
        new BN(1),
        vesting_name,
        investor_wallet_address,
        start,
        end,
        period,
        cliff,
        new BN(10),
        new BN(20),
        {
          accounts: {
            granter: granter.publicKey,
            mint: mint,
            granterToken: granterToken,
            recipient: recipient.publicKey,
            recipientToken: recipientToken,
            vesting: vesting.publicKey,
            escrowVault: escrowVault,
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
            rent: SYSVAR_RENT_PUBKEY
          },
          signers: [granter.payer, vesting]
        }
      );

      console.log("tx: ", tx);
      
      const _escrowVaultToken = await program.provider.connection.getAccountInfo(
        escrowVault
      );

      const _granterToken = await program.provider.connection.getAccountInfo(
        granterToken
      );

      const _vesting = await program.provider.connection.getAccountInfo(
        vesting.publicKey
      );
      
      const _escrowVaultTokenData = common.token.parseTokenAccountData(
        _escrowVaultToken.data
      );

      const _granterTokenData = common.token.parseTokenAccountData(
        _granterToken.data
      );
      
      console.log(
        "deposited during contract creation: ",
        depositedAmount.toNumber(),
        _escrowVaultTokenData.amount
      );

      assert.ok(depositedAmount.toNumber() === _escrowVaultTokenData.amount);

      await program.removeEventListener(listener);
  });

  it("Withdraw", async() => {
      
    await sleep(10000);

    console.log("Withdraw:");
    console.log("recipient token", recipientToken.toBase58());

    let listener = null;
    listener = program.addEventListener("WithdrawEvent", (event, slot) => {
      console.log("slot: ", slot);
      console.log("event data: ",event.data.toNumber());
      console.log("event status: ",event.status);
    });


    const oldEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
      escrowVault
    );

    let oldEscrowVaultAmount;
    if(oldEscrowVaultAccountInfo)
    {
      oldEscrowVaultAmount = common.token.parseTokenAccountData(
        oldEscrowVaultAccountInfo.data
      ).amount;
    }

    const oldRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
      recipientToken
    );

    let oldRecipientTokenAmount;
    if(oldRecipientTokenAccountInfo)
    {
      oldRecipientTokenAmount = common.token.parseTokenAccountData(
        oldRecipientTokenAccountInfo.data
      ).amount;
    }

    const withdrawAmount = new BN(2 * LAMPORTS_PER_SOL);

    console.log(
      "vesting",
      vesting.publicKey.toBase58(),
      "escrowVault",
      escrowVault.toBase58()
      );

    console.log("seed", vesting.publicKey.toBuffer());
    console.log("vesting", vesting.publicKey.toBase58());

    const tx = await program.rpc.withdraw(
      withdrawAmount,
      {
        accounts: {
          recipientToken: recipientToken,
          vesting: vesting.publicKey,
          escrowVault: escrowVault,
          mint: mint,
          tokenProgram: TOKEN_PROGRAM_ID,
          clock: anchor.web3.SYSVAR_CLOCK_PUBKEY
        },
        signers: []
      }
    );

    console.log("tx: ", tx);

    const _vesting = await program.provider.connection.getAccountInfo(
      vesting.publicKey
    );

    const newRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
      recipientToken
    );

    const newRecipientTokenAmount = common.token.parseTokenAccountData(
      newRecipientTokenAccountInfo.data
    ).amount;

    let newEscrowVaultAmount = null;
    const newEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
      escrowVault
    );

    if(newEscrowVaultAccountInfo)
    {
      newEscrowVaultAmount = common.token.parseTokenAccountData(
        newEscrowVaultAccountInfo.data
      ).amount;
    }

    console.log(
      "depositedAmount",
      depositedAmount.toNumber(),
      "withdrawn",
      withdrawAmount.toNumber()
    );

    console.log(
      "escrowVault token balance: previous: ",
      oldEscrowVaultAmount,
      "after: ",
      newEscrowVaultAmount
    );

    console.log(
      "recipient token balance: previous: ",
      oldRecipientTokenAmount,
      "after: ",
      newRecipientTokenAmount
      );

    assert.ok(
      withdrawAmount.eq(new BN(newRecipientTokenAmount - oldRecipientTokenAmount))
    );

    await program.removeEventListener(listener);
  });

  it("Cancel", async() => {

    await sleep(12000);

    let listener = null;
    listener = program.addEventListener("CancelEvent", (event, slot) => {
      console.log("slot: ", slot);
      console.log("event data: ",event.data.toNumber());
      console.log("event status: ",event.status);
    });


    const oldBalance = await provider.connection.getBalance(granter.publicKey);
     
    console.log("\nCancel:");
    const oldGranterAccountInfo = await program.provider.connection.getAccountInfo(
      granterToken
    );

    const oldGranterAmount = common.token.parseTokenAccountData(
      oldGranterAccountInfo.data
    ).amount;

    const oldEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
      escrowVault
    );

    let oldEscrowVaultAmount;
    if(oldEscrowVaultAccountInfo)
    {
      oldEscrowVaultAmount = common.token.parseTokenAccountData(
        oldEscrowVaultAccountInfo.data
      ).amount;
    }

    const oldRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
      recipientToken
    );

    let oldRecipientTokenAmount;
    if(oldRecipientTokenAccountInfo)
    { 
      oldRecipientTokenAmount = common.token.parseTokenAccountData(
        oldRecipientTokenAccountInfo.data
      ).amount;
    }

    const tx = await program.rpc.cancel(
      {
        accounts: {
          granter: granter.publicKey,
          granterToken: granterToken,
          vesting: vesting.publicKey,
          escrowVault: escrowVault,
          mint: mint,
          tokenProgram: TOKEN_PROGRAM_ID
        },
        signers: [granter.payer]
      }
    );

    console.log("tx: ", tx);
    
    let newEscrowVaultAmount = null;
    const newEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
      escrowVault
    );

    if(newEscrowVaultAccountInfo)
    {
      newEscrowVaultAmount = common.token.parseTokenAccountData(
        newEscrowVaultAccountInfo.data
      ).amount;
    }

    const newRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
      recipientToken
    );

    const newRecipientTokenAmount = common.token.parseTokenAccountData(
      newRecipientTokenAccountInfo.data
    ).amount;

    const newGranterAccountInfo = await program.provider.connection.getAccountInfo(
      granterToken
    );

    const newGranterAmount = common.token.parseTokenAccountData(
      newGranterAccountInfo.data
    ).amount;
    
    console.log(
      "old granter",
      oldGranterAmount,
      "old recipientToken",
      oldRecipientTokenAmount,
      "old escrowVault",
      oldEscrowVaultAmount
    );

    console.log(
      "new granter",
      newGranterAmount,
      "new recipientToken",
      newRecipientTokenAmount,
      "new escrowVault",
      newEscrowVaultAmount
    );

    const newBalance = await provider.connection.getBalance(granter.publicKey);
    console.log("Returned:", newBalance - oldBalance);
    assert.ok(newEscrowVaultAmount === null);
    assert.ok((new BN(newRecipientTokenAmount + newGranterAmount)).eq(depositedAmount));

    await program.removeEventListener(listener);


  });
});
