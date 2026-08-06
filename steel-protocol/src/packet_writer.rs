//! # Steel Protocol Packet Writer
//!
//! This module contains the implementation of the packet writer.
/*
Credit to https://github.com/Pumpkin-MC/Pumpkin/ for this implementation.
*/

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use aes::cipher::KeyIvInit;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    packet_traits::EncodedPacket,
    utils::{Aes128Cfb8Enc, PacketError, StreamEncryptor},
};

// raw -> compress -> encrypt
/// A writer that can encrypt data.
pub enum EncryptionWriter<W: AsyncWrite + Unpin> {
    /// A writer that encrypts data.
    Encrypt(Box<StreamEncryptor<W>>),
    /// A writer that does not encrypt data.
    None(W),
}

impl<W: AsyncWrite + Unpin> EncryptionWriter<W> {
    /// Upgrades the writer to encrypt data.
    ///
    /// # Panics
    /// - If the writer is already encrypting data.
    #[must_use]
    pub fn upgrade(self, cipher: Aes128Cfb8Enc) -> Self {
        match self {
            Self::None(stream) => Self::Encrypt(Box::new(StreamEncryptor::new(cipher, stream))),
            Self::Encrypt(_) => panic!("Cannot upgrade a stream that already has a cipher!"),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for EncryptionWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = Pin::new(writer);
                writer.poll_write(cx, buf)
            }
            Self::None(writer) => {
                let writer = Pin::new(writer);
                writer.poll_write(cx, buf)
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = Pin::new(writer);
                writer.poll_flush(cx)
            }
            Self::None(writer) => {
                let writer = Pin::new(writer);
                writer.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = Pin::new(writer);
                writer.poll_shutdown(cx)
            }
            Self::None(writer) => {
                let writer = Pin::new(writer);
                writer.poll_shutdown(cx)
            }
        }
    }
}

/// Encoder: Server -> Client
/// Supports `ZLib` endecoding/compression
/// Supports Aes128 Encryption
pub struct TCPNetworkEncoder<W: AsyncWrite + Unpin> {
    writer: EncryptionWriter<W>,
}

impl<W: AsyncWrite + Unpin> TCPNetworkEncoder<W> {
    /// Creates a new `TCPNetworkEncoder`.
    pub const fn new(writer: W) -> Self {
        Self {
            writer: EncryptionWriter::None(writer),
        }
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    ///
    /// # Panics
    /// - If the stream is already encrypted.
    /// - If the key is invalid.
    pub fn set_encryption(&mut self, key: &[u8; 16]) {
        if matches!(self.writer, EncryptionWriter::Encrypt(_)) {
            panic!("Cannot upgrade a stream that already has a cipher!");
        }
        let cipher = Aes128Cfb8Enc::new_from_slices(key, key).expect("invalid key");
        replace_with::replace_with_or_abort(&mut self.writer, |encoder| encoder.upgrade(cipher));
    }

    /// Writes a packet to the stream.
    ///
    /// # Errors
    /// - If the packet fails to write.
    /// - If the stream fails to flush.
    pub async fn write_packet(&mut self, packet: &EncodedPacket) -> Result<(), PacketError> {
        self.writer
            .write_all(&packet.encoded_data)
            .await
            .map_err(|e| PacketError::EncryptionFailed(e.to_string()))?;

        self.writer
            .flush()
            .await
            .map_err(|e| PacketError::EncryptionFailed(e.to_string()))
    }
}

/// An error that occurs when the compression level is invalid.
#[derive(Error, Debug)]
#[error("Invalid compression Level")]
pub struct CompressionLevelError;
