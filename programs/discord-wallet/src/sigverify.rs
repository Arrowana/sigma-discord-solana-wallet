use pinocchio::{
    account::Ref, error::ProgramError, sysvars::instructions::Instructions, AccountView, Address,
    ProgramResult,
};
use solana_instruction::{syscalls::get_stack_height, TRANSACTION_LEVEL_STACK_HEIGHT};
use solana_sdk_ids::ed25519_program;

const ED25519_SIGNATURE_OFFSETS_START: usize = 2;
const ED25519_SIGNATURE_OFFSETS_SERIALIZED_SIZE: usize = 14;
const ED25519_SIGNATURE_BYTES: usize = 64;
const ED25519_PUBLIC_KEY_BYTES: usize = 32;

#[derive(Clone, Copy)]
struct Ed25519SignatureOffsets {
    signature_offset: u16,
    signature_instruction_index: u16,
    public_key_offset: u16,
    public_key_instruction_index: u16,
    message_data_offset: u16,
    message_data_size: u16,
    message_instruction_index: u16,
}

#[derive(Clone, Copy)]
struct Ed25519Verification<'a> {
    pubkey: [u8; ED25519_PUBLIC_KEY_BYTES],
    message: &'a [u8],
    signature: [u8; ED25519_SIGNATURE_BYTES],
}

impl Ed25519SignatureOffsets {
    fn from_instruction_data(data: &[u8], index: usize) -> Result<Self, ProgramError> {
        let start = ED25519_SIGNATURE_OFFSETS_START
            .checked_add(
                index
                    .checked_mul(ED25519_SIGNATURE_OFFSETS_SERIALIZED_SIZE)
                    .ok_or_else(|| crate::invalid_instruction("ed25519 offsets index overflow"))?,
            )
            .ok_or_else(|| crate::invalid_instruction("ed25519 offsets start overflow"))?;
        let end = start
            .checked_add(ED25519_SIGNATURE_OFFSETS_SERIALIZED_SIZE)
            .ok_or_else(|| crate::invalid_instruction("ed25519 offsets end overflow"))?;
        let bytes = data
            .get(start..end)
            .ok_or_else(|| crate::invalid_instruction("ed25519 offsets out of bounds"))?;

        Ok(Self {
            signature_offset: crate::read_u16_le(bytes, 0)?,
            signature_instruction_index: crate::read_u16_le(bytes, 2)?,
            public_key_offset: crate::read_u16_le(bytes, 4)?,
            public_key_instruction_index: crate::read_u16_le(bytes, 6)?,
            message_data_offset: crate::read_u16_le(bytes, 8)?,
            message_data_size: crate::read_u16_le(bytes, 10)?,
            message_instruction_index: crate::read_u16_le(bytes, 12)?,
        })
    }
}

impl<'a> Ed25519Verification<'a> {
    fn inspect(
        instructions: &'a Instructions<Ref<'a, [u8]>>,
        current_instruction_data: &'a [u8],
        relative_index: i64,
    ) -> Result<Self, ProgramError> {
        let current_index = instructions.load_current_index();
        if current_index == 0 {
            return Err(crate::invalid_instruction(
                "ed25519 verify missing previous instruction",
            ));
        }

        let sigverify_ix = instructions.get_instruction_relative(relative_index)?;
        if sigverify_ix.get_program_id() != &ed25519_program::ID {
            return Err(ProgramError::IncorrectProgramId);
        }

        let sigverify_data = sigverify_ix.get_instruction_data();
        let num_signatures = crate::read_u16_le(sigverify_data, 0)?;
        if num_signatures != 1 {
            return Err(crate::invalid_instruction(
                "ed25519 instruction must contain exactly one signature",
            ));
        }

        let offsets = Ed25519SignatureOffsets::from_instruction_data(sigverify_data, 0)?;
        let signature = Self::get_sigverify_slice(
            sigverify_data,
            offsets.signature_instruction_index,
            offsets.signature_offset,
            ED25519_SIGNATURE_BYTES,
        )?
        .try_into()
        .map_err(|_| crate::invalid_instruction("ed25519 signature length invalid"))?;
        let pubkey = Self::get_sigverify_slice(
            sigverify_data,
            offsets.public_key_instruction_index,
            offsets.public_key_offset,
            ED25519_PUBLIC_KEY_BYTES,
        )?
        .try_into()
        .map_err(|_| crate::invalid_instruction("ed25519 pubkey length invalid"))?;
        let message = Self::get_current_instruction_slice(
            current_instruction_data,
            offsets.message_instruction_index,
            current_index,
            offsets.message_data_offset,
            offsets.message_data_size as usize,
        )?;

        Ok(Self {
            pubkey,
            message,
            signature,
        })
    }

    fn get_sigverify_slice<'b>(
        sigverify_data: &'b [u8],
        instruction_index: u16,
        offset_start: u16,
        size: usize,
    ) -> Result<&'b [u8], ProgramError> {
        if instruction_index != u16::MAX {
            return Err(crate::invalid_instruction(
                "ed25519 signature/pubkey must be embedded in verify instruction",
            ));
        }
        let start = offset_start as usize;
        let end = start
            .checked_add(size)
            .ok_or_else(|| crate::invalid_instruction("ed25519 slice end overflow"))?;
        sigverify_data
            .get(start..end)
            .ok_or_else(|| crate::invalid_instruction("ed25519 slice out of bounds"))
    }

    fn get_current_instruction_slice<'b>(
        current_instruction_data: &'b [u8],
        instruction_index: u16,
        current_index: u16,
        offset_start: u16,
        size: usize,
    ) -> Result<&'b [u8], ProgramError> {
        if instruction_index != current_index {
            return Err(crate::invalid_instruction(
                "ed25519 referenced unsupported instruction index",
            ));
        }
        let start = offset_start as usize;
        let end = start
            .checked_add(size)
            .ok_or_else(|| crate::invalid_instruction("ed25519 slice end overflow"))?;
        current_instruction_data
            .get(start..end)
            .ok_or_else(|| crate::invalid_instruction("ed25519 slice out of bounds"))
    }
}

pub(crate) fn verify_ed25519(
    instructions_sysvar: &AccountView,
    current_instruction_data: &[u8],
    discord_public_key: &Address,
    execute_header_len: usize,
    verified_message_len: usize,
) -> ProgramResult {
    if get_stack_height() != TRANSACTION_LEVEL_STACK_HEIGHT {
        return Err(crate::invalid_instruction(
            "instruction must execute at transaction stack height",
        ));
    }
    let instructions = Instructions::try_from(instructions_sysvar)?;
    let verification = Ed25519Verification::inspect(&instructions, current_instruction_data, -1)?;

    if verification.pubkey != discord_public_key.as_ref() {
        return Err(crate::invalid_instruction(
            "ed25519 discord public key mismatch",
        ));
    }
    if verification.signature.len() != ED25519_SIGNATURE_BYTES {
        return Err(crate::invalid_instruction(
            "ed25519 signature length invalid",
        ));
    }
    if verification.message.as_ptr() != current_instruction_data[execute_header_len..].as_ptr()
        || verification.message.len() != verified_message_len
    {
        return Err(crate::invalid_instruction(
            "ed25519 offsets or message binding invalid",
        ));
    }
    if current_instruction_data.len() != execute_header_len + verified_message_len {
        return Err(crate::invalid_instruction(
            "current instruction payload length mismatch",
        ));
    }

    Ok(())
}
