const BufferLayout = require("buffer-layout");
const { PublicKey } = require("@solana/web3.js");
const anchor = require("@project-serum/anchor");
const { BN } = anchor;

const LE = 'le'; //little endian

const VestingDataLayout = BufferLayout.struct([
    BufferLayout.blob(8, "Unknown"),
    BufferLayout.blob(4, "magic"),
    BufferLayout.blob(4, "version"),
    BufferLayout.blob(1, "escrowVaultBump"),
    BufferLayout.blob(1, "vestingBump"),
    BufferLayout.blob(8, "vestingId"),
    BufferLayout.blob(32, "vestingName"),
    BufferLayout.blob(64, "investorWalletAddress"),
    BufferLayout.blob(8, "withdrawnAmount"),
    BufferLayout.blob(8, "remainingAmount"),
    BufferLayout.blob(8, "totalAmount"),
    BufferLayout.blob(32, "granter"),
    BufferLayout.blob(32, "granterToken"),
    BufferLayout.blob(32, "recipient"),
    BufferLayout.blob(32, "recipientToken"),
    BufferLayout.blob(32, "mint"),
    BufferLayout.blob(32, "escrowVault"),
    BufferLayout.blob(8, "createTs"),
    BufferLayout.blob(8, "startTs"),
    BufferLayout.blob(8, "endTs"),
    BufferLayout.blob(8, "accountingTs"),
    BufferLayout.blob(8, "lastWithdrawnAt"),
    BufferLayout.blob(8, "period"),
    BufferLayout.blob(8, "cliff"),
    BufferLayout.blob(8, "cliffReleaseRate"),
    BufferLayout.blob(8, "cliffAmount"),
    BufferLayout.blob(8, "tgeReleaseRate"),
    BufferLayout.blob(8, "tgeAmount"),
    BufferLayout.blob(8, "periodicUnlockAmount"),
]);

function decode_vesting_data(buf) {
    let raw = VestingDataLayout.decode(buf);
    return {
        magic: new BN(raw.magic, LE),
        version: new BN(raw.version, LE),
        escrowVaultBump: raw.escrowVaultBump.readUInt8(),
        vestingBump: raw.vestingBump.readUInt8(),
        vestingId: new BN(raw.vestingId, LE),
        vestingName: new String(raw.vestingName),
        investorWalletAddress: new String(raw.investorWalletAddress),
        withdrawnAmount: new BN(raw.withdrawnAmount, LE),
        remainingAmount: new BN(raw.remainingAmount, LE),
        totalAmount: new BN(raw.totalAmount, LE),
        granter: new PublicKey(raw.granter),
        granterToken: new PublicKey(raw.granterToken),
        recipient: new PublicKey(raw.recipient),
        recipientToken: new PublicKey(raw.recipientToken),
        mint: new PublicKey(raw.mint),
        escrowVault: new PublicKey(raw.escrowVault),
        createTs: new BN(raw.createTs, LE),
        startTs: new BN(raw.startTs, LE),
        endTs: new BN(raw.endTs, LE),
        accountingTs: new BN(raw.accountingTs, LE),
        lastWithdrawnAt: new BN(raw.lastWithdrawnAt, LE),
        period: new BN(raw.period, LE),
        cliff: new BN(raw.cliff, LE),
        cliffReleaseRate: new BN(raw.cliffReleaseRate, LE),
        cliffAmount: new BN(raw.cliffAmount, LE),
        tgeReleaseRate: new BN(raw.tgeReleaseRate, LE),
        tgeAmount: new BN(raw.tgeAmount, LE),
        periodicUnlockAmount: new BN(raw.periodicUnlockAmount, LE),
    }
}

exports.decode = decode_vesting_data;