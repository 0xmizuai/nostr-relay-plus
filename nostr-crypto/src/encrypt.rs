use anyhow::{anyhow, Result};
use xsalsa20poly1305::{aead::Aead, KeyInit, XSalsa20Poly1305, NONCE_SIZE};

pub fn encrypt_xsalsa20poly1305(msg: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let nonce = rand::random::<[u8; NONCE_SIZE]>();
    let encryptor = XSalsa20Poly1305::new(key.into());

    // SAFETY: encryptor might return error when the ineternal buffer overflows
    // which won't happen in this case
    let cipher_text = encryptor.encrypt(&nonce.into(), msg)
        .expect("encryption to be successful");

    [nonce.to_vec(), cipher_text].concat()
}

pub fn decrypt_xsalsa20poly1305(cipher: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let nonce = &cipher[..NONCE_SIZE];
    let cipher_text = &cipher[NONCE_SIZE..];

    let decryptor = XSalsa20Poly1305::new(key.into());

    // SAFETY: decryptor might return error when the ineternal buffer overflows
    // which won't happen in this case
    let plain_text = decryptor.decrypt(nonce.into(), cipher_text)
        .map_err(|e| anyhow!("decryption error {}", e.to_string()))?;

    Ok(plain_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_encryption() {
        let msg = b"hello world";
        let key = [0u8; 32];
        let cipher_text = encrypt_xsalsa20poly1305(msg, &key);
        let plain_text = decrypt_xsalsa20poly1305(&cipher_text, &key).unwrap();

        assert_eq!(msg, &plain_text[..]);
    }

    #[test]
    fn test_invalid_encryption() {
        let msg = b"hello world";
        let key = [0u8; 32];
        let cipher_text = encrypt_xsalsa20poly1305(msg, &key);
        let plain_text = decrypt_xsalsa20poly1305(&cipher_text[10..], &[0u8; 32]);
        assert!(plain_text.is_err());
    }

}