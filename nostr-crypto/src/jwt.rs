use anyhow::Result;
use num::BigUint;

use crate::hash::{keccak_digest, sha256_hash_digests};
use crate::rsa::rsa_verify;
use crate::utils::{bits_to_bytes, u64_to_bits};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    #[allow(dead_code)]
    alg: String,
    kid: String,
    typ: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenId3Claim {
    iss: String,
    nonce: String,
    aud: String,
    sub: String,
    iat: u64,
}

#[derive(Debug, Clone)]
pub struct JwtWitness {
    pub jwt: String,

    pub jwt_encoded: Vec<u8>,
    pub jwt_header: Vec<u8>,
    pub jwt_payload: Vec<u8>,
    
    pub jwt_signature: BigUint,
    pub jwt_pub_key: BigUint,
    
    pub kid: Vec<u8>, pub nonce: Vec<u8>, pub iat: Vec<u8>, pub sub: Vec<u8>,
}

impl JwtWitness {

    pub fn from_jwt(jwt: &str, pub_key: &str) -> Result<Self> {
        let jwt_parts = jwt.split('.').collect::<Vec<_>>();
        let header_raw = jwt_parts[0];
        let payload_raw = jwt_parts[1];
        let signature = jwt_parts[2];

        let jwt_str_without_sig = format!("{}.{}", header_raw, payload_raw);

        let header = base64::decode(header_raw).unwrap();
        let payload = base64::decode(payload_raw).unwrap();

        let signature = base64::decode_config(signature, base64::URL_SAFE_NO_PAD).unwrap();
        let pub_key = base64::decode_config(pub_key, base64::URL_SAFE_NO_PAD).unwrap();

        let header: Header = serde_json::from_slice(&header).unwrap();
        let payload: OpenId3Claim = serde_json::from_slice(&payload).unwrap();
        
        let digest = sha256_hash_digests(
            &jwt_str_without_sig.as_bytes()
        );

        let signature = BigUint::from_bytes_be(&signature);
        let pub_key = BigUint::from_bytes_be(&pub_key);

        if !rsa_verify(&signature, &pub_key, &digest) {
            return Err(anyhow::anyhow!("Invalid signature"));
        }

        let witness = JwtWitness {
            jwt: jwt.to_string(),
            jwt_encoded: jwt_str_without_sig.as_bytes().to_vec(),
            jwt_header: serde_json::to_vec(&header)?,
            jwt_payload: serde_json::to_vec(&payload)?,
            jwt_signature: signature,
            jwt_pub_key: pub_key,

            nonce: hex::decode(payload.nonce.clone())?.to_vec(),
            sub: payload.sub.as_bytes().to_vec(),
            iat: format!("{}", payload.iat).as_bytes().to_vec(),
            kid: header.kid.as_bytes().to_vec(),
        };

        Ok(witness)
    }

    pub fn get_iat(&self) -> u64 {
        let mut f = 0;
        for i in 0..10 {
            f += (self.iat[i] - 48) as u32 * 10_u32.pow(9 - i as u32);
        }

        f as u64
    }

    pub fn get_nonce(&self) -> Vec<u8> {
        self.nonce.clone()
    }

    pub fn get_nonce_str(&self) -> String {
        hex::encode(self.nonce.clone())
    }

    pub fn get_id_hash(&self) -> [u8; 32] {
        keccak_digest(&self.sub)
    }

    pub fn get_id_hash_str(&self) -> String {
        hex::encode(Self::get_id_hash(&self))
    }

    pub fn output_hash_preimage(&self) -> [u8; 104] {
        let ras_pub_key_hash =  keccak_digest(&self.jwt_pub_key.to_bytes_be());
        let id_hash = keccak_digest(&self.sub);
        let nonce = self.nonce.clone();

        // iat 
        assert_eq!(self.iat.len(), 10);
        let mut f = 0;
        for i in 0..10 {
            f += (self.iat[i] - 48) as u64 * 10_u32.pow(9 - i as u32) as u64;
        }

        let f_bits = u64_to_bits(f);
        let mut output = [ras_pub_key_hash, id_hash].concat();
        output.extend(nonce);
        output.extend(bits_to_bytes(&f_bits));
        output.try_into().unwrap()
    }

    pub fn output_hash(&self) -> [u8; 32] {
        let preimage = self.output_hash_preimage();
        keccak_digest(&preimage)
    }

    pub fn output_hash_str(&self) -> String {
        hex::encode(self.output_hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt() {

        let x = JwtWitness::from_jwt( 
            "eyJhbGciOiJSUzI1NiIsImtpZCI6IjhERTZEMTYyREE4NTk4ZGFlRTE3Nzk0NTA5ODNCOTYwYzI4Mzc2N2UiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2FwaS5vcGVuaWQzLnh5eiIsIm5vbmNlIjoiZTkxMTg4ZGIxYmMzNWVhNGU5ZmZlYzc4MDgxNTNlNTM5NGZkNWU0NGM4YTIwZDhmZGQ0NzMxYTJhODg1YzNjNSIsImF1ZCI6InprLWp3dCIsInN1YiI6Imdvb2dsZS0xMDIyNDM3Njk5OTMzNTU5OTIwNzkiLCJpYXQiOjE3MDg0NDE2NzN9.TYhRsH3wzLSwVPz0brotHp7W5XuHz9ZrjMUYNJ1RWaIoWoiUpFZcJlSFmfG3oy_sJ7Mb_MOjpv6kvuREFpRd-2YTaxS9K3ZQ1eg8N5Lw8bYFH3uqyBVwG1zO-vhGlwTXvvhiYwwP3JwpvaPhccFWepkT2dkKCzhVbhZ8bZ58ExqbfT9MngfdaCVNns7lXc-mBh0JaJzUWwp7Ir6Ok3A4A0drh6jLDUxzojZ4EsfRJC4pcWXuzPSFIfFWQ7kC5_5B3BBxFCxwV8dHnwjXUclRmBSk_YpJEZ4kbjo-18xIlGUdvxx3Ing2eBCHAkHnBmd5etcer9L-gdmIF1zZPho6VQ",
            "r6dnzQXiymQg19FInr31eQjwtm0bbRC5LHx7zaENF5PXnGy56bPOMXFClzODr07WmVBd-T73xcAwI0CX6J_a9zRPhwncErT2KLlg0X7pzQp4NIn48yYnojVNKL-kp1yxOF5ySIOHVUhuR2AUIDYzQqYUGYC-LHmIKPsv7vn-1z3BfPpvuszKlaEWju6g6r9UTHsDBDbB4WhobWRRNC7F0ipKX-n7w6j_edfBImAlZ1-FSExyt1cDEfofwpexLDVn66Mt7gsNF4KhQpEGlhZBQQKgbYiDHgUQ5Yx2BJOnrL9mi-rZJ8augIyl0lQiIoAFSnWv5xmTSfrsWFDWQXM_OQ"
        ).expect("successful parse");

        println!("{:?}", x.output_hash_str());
    }
}
