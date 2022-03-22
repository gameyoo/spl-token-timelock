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

const { BN } = anchor;
const {
    SystemProgram,
    Keypair,
    PublicKey,
    SYSVAR_RENT_PUBKEY,
    LAMPORTS_PER_SOL
} = anchor.web3;

const {decode} = require("./layout");

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
            authority, [signer],
            amount
        )
    );
    await provider.send(tx, [signer]);
}

describe('spl-token-timelock', () => {

    // Configure the client to use the local cluster.
    const provider = anchor.Provider.env();
    anchor.setProvider(provider);

    // Specify the SplTokenTimelock to test/use.
    const program = anchor.workspace.SplTokenTimelock;
    const DECIMALS = 9;

    // Timestamp (in seconds) when the stream/token vesting starts,Divide by 1000 since Unix timestamp is seconds.
    //const start = new BN(+new Date() / 1000 - 5);    //Verify bypass_timestamp_check param that is works.
    const start = new BN(1647705600);

    // Timestamp (in seconds) of cliff.
    const cliff = new BN(0);

    // Timestamp (in seconds) when the stream/token vesting end, +60 seconds.
    const end = new BN(1655654399);

    // In seconds.
    const period = new BN(86400);

    // Amount to deposit.
    const depositedAmount = new BN(428.268583 * LAMPORTS_PER_SOL);

    const vestingId = 100801;

    let mint;
    let granter = provider.wallet;
    let recipientToken;
    let config;
    let configBump;
    let paymentVault;
    let paymentVaultBump;
    let vesting;
    let vestingBump;
    let escrowVault;
    let escrowVaultBump;

    //let recipient = Keypair.generate();
    let recipient = new PublicKey("oRy4jmWm5MNuKTRaBn1t3tGmL9ERrDzoS2y8P19bM52");

    before(async () => {

        // Create token mint.
        // mint = await common.createMint(
        //     provider,
        //     granter.publicKey,
        //     DECIMALS
        // );

        mint = new PublicKey("GYCVdmDthkf3jSz5ns6fkzCmHub7FSZxjVCfbfGqkH7P");
        //let recipient = new PublicKey("55YsfAvxUi2RkGye5AS3hH6kvoz3Bf1hoQaoX4VVKaF7");
        //let recipient = new PublicKey("3tTiZUz1GsbfUTYq7PJqeUGFWukqZvT19auTFtJN4sXn");

        console.log("mint: ", mint);

        [config, configBump] = await PublicKey.findProgramAddress(
            [Buffer.from("gyc_timelock")],
            program.programId
        );

        [paymentVault, paymentVaultBump] = await PublicKey.findProgramAddress(
            [config.toBuffer()],
            program.programId
        );

        [vesting, vestingBump] = await PublicKey.findProgramAddress(
            // [vestingId.toString(), recipient.toBuffer()],
            [
                Buffer.from(String(vestingId)),
                recipient.toBuffer()
            ],
            program.programId
        );

        // Get escrow vault account address, it's PDA.
        [escrowVault, escrowVaultBump] = await PublicKey.findProgramAddress(
            [vesting.toBuffer()],
            program.programId
        );

        // Get associated token account address of escrow vault.
        recipientToken = await Token.getAssociatedTokenAddress(
            ASSOCIATED_TOKEN_PROGRAM_ID,
            TOKEN_PROGRAM_ID,
            mint,
            recipient
        );

        console.log(`Before: `);
        console.log(`programId: ${program.programId.toBase58()}
signer wallet: ${granter.publicKey.toBase58()}
mint: ${mint.toBase58()}
config: ${config.toBase58()}
configBump: ${configBump}
paymentVault: ${paymentVault.toBase58()}
paymentVaultBump: ${paymentVaultBump}
`);

        // const vestingAccount = await program.account.vesting.fetch(vesting);
        // console.log("fetch vesting: ", vestingAccount);

    });

    it("Initialize", async () => {

        console.log(`Initialize: `);

        const tx = await program.rpc.initialize(
            configBump,
            paymentVaultBump, {
            accounts: {
                signer: granter.publicKey,
                authority: granter.publicKey,
                mint: mint,
                paymentVault: paymentVault,
                config: config,
                tokenProgram: TOKEN_PROGRAM_ID,
                associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
                systemProgram: SystemProgram.programId,
                rent: SYSVAR_RENT_PUBKEY,
            },
            signer: [granter.payer]
        });

        console.log(`tx: ${tx}`);

        //Mint some tokens to granter.
        await mintTo(
            provider,
            mint,
            paymentVault,
            granter.publicKey,
            10000 * LAMPORTS_PER_SOL,
            granter.payer
        );

        const _paymentVault = await program.provider.connection.getAccountInfo(
            paymentVault
        );

        const _paymentVaultData = common.token.parseTokenAccountData(
            _paymentVault.data
        );

        console.log(`PaymentVault Token Amount: ${_paymentVaultData.amount}`);
    });

    it("Create vesting", async () => {

        console.log(`Create vesting: `);

        // Listen CreateVesting event of on-chain program.
        let listener = null;
        listener = program.addEventListener("CreateVestingEvent", (event, slot) => {
            console.log("slot: ", slot);
            console.log("event data: ", event.data.toNumber());
            console.log("event status: ", event.status);
        });

        // Create vesting by Invoke createVesting instruction of on-chain program.
        let vesting_name = nacl.util.decodeUTF8("DaoLaunch_38");
        let investor_wallet_address = nacl.util.decodeUTF8("55YsfAvxUi2RkGye5AS3hH6kvoz3Bf1hoQaoX4VVKaF7");
        const tx = await program.rpc.createVesting(
            depositedAmount,
            escrowVaultBump,
            vestingBump,
            new BN(vestingId),
            vesting_name,
            investor_wallet_address,
            start,
            end,
            period,
            cliff,
            new BN(0),
            new BN(20),
            true, {
            accounts: {
                signer: granter.publicKey,
                paymentVault: paymentVault,
                config: config,
                recipient: recipient,
                recipientToken: recipientToken,
                vesting: vesting,
                escrowVault: escrowVault,
                mint: mint,
                tokenProgram: TOKEN_PROGRAM_ID,
                associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
                systemProgram: SystemProgram.programId,
                clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
                rent: SYSVAR_RENT_PUBKEY
            },
            signers: [granter.payer]
        }
        );

        console.log(`tx: ${tx}`);

        const _escrowVaultToken = await program.provider.connection.getAccountInfo(
            escrowVault
        );

        const _paymentVault = await program.provider.connection.getAccountInfo(
            paymentVault
        );

        const _vesting = await program.provider.connection.getAccountInfo(
            vesting
        );

//         console.log("vesting: ", _vesting);
//         let vesting_data = decode(_vesting.data);
//         //console.log(`vesting_data: ${vesting_data}`);
//         console.log("vesting_data: ", vesting_data);
//         console.log(`
// ${vesting_data.granter.toBase58()}
// ${vesting_data.granterToken.toBase58()}
// ${vesting_data.recipient.toBase58()}
// ${vesting_data.recipientToken.toBase58()}
// ${vesting_data.mint.toBase58()}
// ${vesting_data.escrowVault.toBase58()}
// `);

        const vestingAccount = await program.account.vesting.fetch(vesting);
        console.log("fetch vesting: ", vestingAccount);

        const _escrowVaultTokenData = common.token.parseTokenAccountData(
            _escrowVaultToken.data
        );

        const _paymentVaultData = common.token.parseTokenAccountData(
            _paymentVault.data
        );

        console.log(`deposited during vesting creation: ${depositedAmount.toNumber()}
escrowVault: ${_escrowVaultTokenData.amount}`);

        // Verify.
        assert.ok(depositedAmount.toNumber() === _escrowVaultTokenData.amount);

        await program.removeEventListener(listener);

    });

    it("Withdraw", async () => {

        await sleep(2000);

        console.log(`Withdraw: `);
        console.log(`recipient token: ${recipientToken.toBase58()}`);

        // Listen withdraw event of on-chain program.
        let listener = null;
        listener = program.addEventListener("WithdrawEvent", (event, slot) => {
            console.log("slot: ", slot);
            console.log("event data: ", event.data.toNumber());
            console.log("event status: ", event.status);
        });

        const oldEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
            escrowVault
        );

        let oldEscrowVaultAmount;
        if (oldEscrowVaultAccountInfo) {
            oldEscrowVaultAmount = common.token.parseTokenAccountData(
                oldEscrowVaultAccountInfo.data
            ).amount;
        }

        const oldRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
            recipientToken
        );

        let oldRecipientTokenAmount;
        if (oldRecipientTokenAccountInfo) {
            oldRecipientTokenAmount = common.token.parseTokenAccountData(
                oldRecipientTokenAccountInfo.data
            ).amount;
        }

        const withdrawAmount = new BN(85.653716);

        console.log(`vesting: ${vesting.toBase58()}
escrowVault: ${escrowVault.toBase58()}`);

        // Withdraw from escrow vault account by Invoke withdraw instruction of on-chain program.
        const tx = await program.rpc.withdraw(
            withdrawAmount, {
            accounts: {
                recipientToken: recipientToken,
                vesting: vesting,
                escrowVault: escrowVault,
                mint: mint,
                tokenProgram: TOKEN_PROGRAM_ID,
                clock: anchor.web3.SYSVAR_CLOCK_PUBKEY
            },
            signers: []
        }
        );

        console.log(`tx: ${tx}`);

        /*
            Get and print updated state of the accounts.
        */
        const _vesting = await program.provider.connection.getAccountInfo(
            vesting
        );

        console.log("vesting: ", _vesting);
        let vesting_data = decode(_vesting.data);
        //console.log(`vesting_data: ${vesting_data}`);
        console.log("vesting_data: ", vesting_data);
        console.log(`
${vesting_data.granter.toBase58()}
${vesting_data.granterToken.toBase58()}
${vesting_data.recipient.toBase58()}
${vesting_data.recipientToken.toBase58()}
${vesting_data.mint.toBase58()}
${vesting_data.escrowVault.toBase58()}
`);

        const vestingAccount = await program.account.vesting.fetch(vesting);
        console.log("fetch vesting: ", vestingAccount);

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

        if (newEscrowVaultAccountInfo) {
            newEscrowVaultAmount = common.token.parseTokenAccountData(
                newEscrowVaultAccountInfo.data
            ).amount;
        }

        console.log(`depositedAmount: ${depositedAmount.toNumber()} withdrawn: ${withdrawAmount.toNumber()}`);
        console.log(`escrowVault token balance: previous: ${oldEscrowVaultAmount} after: ${newEscrowVaultAmount}`);
        console.log(`recipient token balance: previous: ${oldRecipientTokenAmount} after: ${newRecipientTokenAmount}`);

        // Verify.
        assert.ok(
            withdrawAmount.eq(new BN(newRecipientTokenAmount - oldRecipientTokenAmount))
        );

        await program.removeEventListener(listener);
    });

//     it("Cancel", async () => {

//         await sleep(12000);

//         // Listen cancel event of on-chain program.
//         let listener = null;
//         listener = program.addEventListener("CancelEvent", (event, slot) => {
//             console.log("slot: ", slot);
//             console.log("event data: ", event.data.toNumber());
//             console.log("event status: ", event.status);
//         });

//         const oldBalance = await provider.connection.getBalance(granter.publicKey);

//         console.log(`Cancel: `);
//         const oldPaymentVaultInfo = await program.provider.connection.getAccountInfo(
//             paymentVault
//         );

//         const oldPaymentVaultAmount = common.token.parseTokenAccountData(
//             oldPaymentVaultInfo.data
//         ).amount;

//         const oldEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
//             escrowVault
//         );

//         let oldEscrowVaultAmount;
//         if (oldEscrowVaultAccountInfo) {
//             oldEscrowVaultAmount = common.token.parseTokenAccountData(
//                 oldEscrowVaultAccountInfo.data
//             ).amount;
//         }

//         const oldRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
//             recipientToken
//         );

//         let oldRecipientTokenAmount;
//         if (oldRecipientTokenAccountInfo) {
//             oldRecipientTokenAmount = common.token.parseTokenAccountData(
//                 oldRecipientTokenAccountInfo.data
//             ).amount;
//         }

//         // Cancel vesting by Invoke cancel instruction of on-chain program.
//         const tx = await program.rpc.cancel({
//             accounts: {
//                 signer: granter.publicKey,
//                 paymentVault: paymentVault,
//                 config: config,
//                 vesting: vesting,
//                 escrowVault: escrowVault,
//                 mint: mint,
//                 tokenProgram: TOKEN_PROGRAM_ID
//             },
//             signers: [granter.payer]
//         });

//         console.log(`tx: ${tx}`);

//         /*
//             Get and print the relevant account information and verify it accordingly.
//         */
//         let newEscrowVaultAmount = null;
//         const newEscrowVaultAccountInfo = await program.provider.connection.getAccountInfo(
//             escrowVault
//         );

//         if (newEscrowVaultAccountInfo) {
//             newEscrowVaultAmount = common.token.parseTokenAccountData(
//                 newEscrowVaultAccountInfo.data
//             ).amount;
//         }

//         const newRecipientTokenAccountInfo = await program.provider.connection.getAccountInfo(
//             recipientToken
//         );

//         const newRecipientTokenAmount = common.token.parseTokenAccountData(
//             newRecipientTokenAccountInfo.data
//         ).amount;

//         const newPaymentVaultInfo = await program.provider.connection.getAccountInfo(
//             paymentVault
//         );

//         const newPaymentVaultAmount = common.token.parseTokenAccountData(
//             newPaymentVaultInfo.data
//         ).amount;

//         console.log(`oldPaymentVault: ${oldPaymentVaultAmount}
// old recipientToken: ${oldRecipientTokenAmount}
// old escrowVault: ${oldEscrowVaultAmount}`);

//         console.log(`newPaymentVault: ${newPaymentVaultAmount}
// new recipientToken: ${newRecipientTokenAmount}
// new escrowVault: ${newEscrowVaultAmount}`);

//         const newBalance = await provider.connection.getBalance(granter.publicKey);
//         console.log("Returned:", newBalance - oldBalance);
//         assert.ok(newEscrowVaultAmount === null);
//         assert.ok((new BN(newRecipientTokenAmount + newPaymentVaultAmount)).eq(depositedAmount));

//         await program.removeEventListener(listener);


//     });
});