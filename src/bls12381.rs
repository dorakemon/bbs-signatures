/*
 * Copyright 2020 - MATTR Limited
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::utils::set_panic_hook;

use crate::{BbsVerifyResponse, PoKOfSignatureProofMultiWrapper, PoKOfSignatureProofWrapper};
use bbs::prelude::*;
use pairing_plus::{
    bls12_381::{Bls12, Fr, G1, G2},
    hash_to_field::BaseFromRO,
    serdes::SerDes,
    CurveProjective,
};
use rand::{thread_rng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
    iter::FromIterator,
};
use wasm_bindgen::prelude::*;

use itertools::multizip;

wasm_impl!(
    /// Convenience struct for interfacing with JS.
    /// Option allows both of the keys to be JS::null
    /// or only one of them set.
    #[allow(non_snake_case)]
    #[derive(Debug, Deserialize, Serialize)]
    BlsKeyPair,
    publicKey: Option<Vec<u8>>,
    secretKey: Option<SecretKey>
);

wasm_impl!(
    Bls12381ToBbsRequest,
    keyPair: BlsKeyPair,
    messageCount: usize
);

wasm_impl!(
    BbsKeyPair,
    publicKey: PublicKey,
    secretKey: Option<SecretKey>,
    messageCount: usize
);

wasm_impl!(
    BlsBbsSignRequest,
    keyPair: BlsKeyPair,
    messages: Vec<Vec<u8>>
);

wasm_impl!(
    BlsBbsVerifyRequest,
    publicKey: DeterministicPublicKey,
    signature: Signature,
    messages: Vec<Vec<u8>>
);

wasm_impl!(
    BlsCreateProofRequest,
    signature: Signature,
    publicKey: DeterministicPublicKey,
    messages: Vec<Vec<u8>>,
    revealed: Vec<usize>,
    nonce: Vec<u8>
);

wasm_impl!(
    BlsVerifyProofContext,
    proof: PoKOfSignatureProofWrapper,
    publicKey: DeterministicPublicKey,
    messages: Vec<Vec<u8>>,
    nonce: Vec<u8>
);

wasm_impl!(
    BlsCreateProofMultiRequest,
    signature: Vec<Signature>,
    publicKey: Vec<DeterministicPublicKey>,
    messages: Vec<Vec<Vec<u8>>>,
    revealed: Vec<Vec<usize>>,
    nonce: Vec<u8>,
    equivs: Vec<Vec<(usize, usize)>>
);

wasm_impl!(
    BlsVerifyProofMultiContext,
    proof: Vec<PoKOfSignatureProofWrapper>,
    publicKey: Vec<DeterministicPublicKey>,
    messages: Vec<Vec<Vec<u8>>>,
    nonce: Vec<u8>
);

/// Generate a BLS 12-381 key pair.
///
/// * seed: UIntArray with 32 element
///
/// returned vector is the concatenation of first the private key (32 bytes)
/// followed by the public key (96) bytes.
#[wasm_bindgen(js_name = generateBls12381G2KeyPair)]
pub async fn bls_generate_g2_key(seed: Option<Vec<u8>>) -> Result<JsValue, JsValue> {
    set_panic_hook();
    Ok(bls_generate_keypair::<G2>(seed))
}

/// Generate a BLS 12-381 key pair.
///
/// * seed: UIntArray with 32 element
///
/// returned vector is the concatenation of first the private key (32 bytes)
/// followed by the public key (48) bytes.
#[wasm_bindgen(js_name = generateBls12381G1KeyPair)]
pub async fn bls_generate_g1_key(seed: Option<Vec<u8>>) -> Result<JsValue, JsValue> {
    set_panic_hook();
    Ok(bls_generate_keypair::<G1>(seed))
}

/// Get the BBS public key associated with the private key
#[wasm_bindgen(js_name = bls12381toBbs)]
pub async fn bls_to_bbs_key(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: Bls12381ToBbsRequest = request.try_into()?;
    if request.messageCount == 0 {
        return Err(JsValue::from_str("Failed to convert key"));
    }
    if let Some(dpk_bytes) = request.keyPair.publicKey {
        let dpk = DeterministicPublicKey::from(array_ref![dpk_bytes, 0, G2_COMPRESSED_SIZE]);
        let pk = dpk.to_public_key(request.messageCount)?;
        let key_pair = BbsKeyPair {
            publicKey: pk,
            secretKey: request.keyPair.secretKey,
            messageCount: request.messageCount,
        };
        Ok(serde_wasm_bindgen::to_value(&key_pair).unwrap())
    } else if let Some(s) = request.keyPair.secretKey {
        let (dpk, sk) = DeterministicPublicKey::new(Some(KeyGenOption::FromSecretKey(s)));
        let pk = dpk.to_public_key(request.messageCount)?;
        let key_pair = BbsKeyPair {
            publicKey: pk,
            secretKey: Some(sk),
            messageCount: request.messageCount,
        };
        Ok(serde_wasm_bindgen::to_value(&key_pair).unwrap())
    } else {
        Err(JsValue::from_str("No key is specified"))
    }
}

/// Signs a set of messages with a BLS 12-381 key pair and produces a BBS signature
#[wasm_bindgen(js_name = blsSign)]
pub async fn bls_sign(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlsBbsSignRequest = request.try_into()?;
    let dpk_bytes = request.keyPair.publicKey.unwrap();
    let dpk = DeterministicPublicKey::from(array_ref![dpk_bytes, 0, G2_COMPRESSED_SIZE]);
    let pk_res = dpk.to_public_key(request.messages.len());
    let pk;
    match pk_res {
        Err(_) => return Err(JsValue::from_str("Failed to convert key")),
        Ok(p) => pk = p,
    };
    if request.keyPair.secretKey.is_none() {
        return Err(JsValue::from_str("Failed to sign"));
    }
    let messages: Vec<SignatureMessage> = request
        .messages
        .iter()
        .map(|m| SignatureMessage::hash(m))
        .collect();
    match Signature::new(
        messages.as_slice(),
        &request.keyPair.secretKey.unwrap(),
        &pk,
    ) {
        Ok(sig) => Ok(serde_wasm_bindgen::to_value(&sig).unwrap()),
        Err(e) => Err(JsValue::from(&format!("{:?}", e))),
    }
}

/// Verifies a BBS+ signature for a set of messages with a with a BLS 12-381 public key
#[wasm_bindgen(js_name = blsVerify)]
pub async fn bls_verify(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let res = request.try_into();
    let result: BlsBbsVerifyRequest;
    match res {
        Ok(r) => result = r,
        Err(e) => {
            return Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
                verified: false,
                error: Some(format!("{:?}", e)),
            })
            .unwrap())
        }
    };
    if result.messages.is_empty() {
        return Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: false,
            error: Some("Messages cannot be empty".to_string()),
        })
        .unwrap());
    }
    let pk = result.publicKey.to_public_key(result.messages.len())?;
    let messages: Vec<SignatureMessage> = result
        .messages
        .iter()
        .map(|m| SignatureMessage::hash(m))
        .collect();
    match result.signature.verify(messages.as_slice(), &pk) {
        Err(e) => Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: false,
            error: Some(format!("{:?}", e)),
        })
        .unwrap()),
        Ok(b) => Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: b,
            error: None,
        })
        .unwrap()),
    }
}

/// Creates a BBS+ PoK
#[wasm_bindgen(js_name = blsCreateProof)]
pub async fn bls_create_proof(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlsCreateProofRequest = request.try_into()?;
    if request.revealed.iter().any(|r| *r > request.messages.len()) {
        return Err(JsValue::from("revealed value is out of bounds"));
    }
    let pk = request.publicKey.to_public_key(request.messages.len())?;
    let revealed: BTreeSet<usize> = BTreeSet::from_iter(request.revealed.into_iter());
    let mut messages = Vec::new();
    for i in 0..request.messages.len() {
        if revealed.contains(&i) {
            messages.push(ProofMessage::Revealed(SignatureMessage::hash(
                &request.messages[i],
            )));
        } else {
            messages.push(ProofMessage::Hidden(HiddenMessage::ProofSpecificBlinding(
                SignatureMessage::hash(&request.messages[i]),
            )));
        }
    }
    match PoKOfSignature::init(&request.signature, &pk, messages.as_slice()) {
        Err(e) => return Err(JsValue::from(&format!("{:?}", e))),
        Ok(pok) => {
            let mut challenge_bytes = pok.to_bytes();
            if request.nonce.is_empty() {
                challenge_bytes.extend_from_slice(&[0u8; FR_COMPRESSED_SIZE]);
            } else {
                let nonce = ProofNonce::hash(&request.nonce);
                challenge_bytes.extend_from_slice(nonce.to_bytes_uncompressed_form().as_ref());
            }
            let challenge_hash = ProofChallenge::hash(&challenge_bytes);
            match pok.gen_proof(&challenge_hash) {
                Ok(proof) => {
                    let out =
                        PoKOfSignatureProofWrapper::new(request.messages.len(), &revealed, proof);
                    Ok(serde_wasm_bindgen::to_value(&out).unwrap())
                }
                Err(e) => Err(JsValue::from(&format!("{:?}", e))),
            }
        }
    }
}

/// Verify a BBS+ PoK
#[wasm_bindgen(js_name = blsVerifyProof)]
pub async fn bls_verify_proof(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let res = serde_wasm_bindgen::from_value::<BlsVerifyProofContext>(request);
    let request: BlsVerifyProofContext;
    match res {
        Ok(r) => request = r,
        Err(e) => {
            return Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
                verified: false,
                error: Some(format!("{:?}", e)),
            })
            .unwrap())
        }
    };

    let nonce = if request.nonce.is_empty() {
        ProofNonce::default()
    } else {
        ProofNonce::hash(&request.nonce)
    };
    let message_count = u16::from_be_bytes(*array_ref![request.proof.bit_vector, 0, 2]) as usize;
    let pk = request.publicKey.to_public_key(message_count)?;
    let messages = request.messages.clone();
    let (revealed, proof) = request.proof.unwrap();
    let proof_request = ProofRequest {
        revealed_messages: revealed,
        verification_key: pk,
    };

    let revealed_vec = proof_request
        .revealed_messages
        .iter()
        .collect::<Vec<&usize>>();
    let mut revealed_messages = BTreeMap::new();
    for i in 0..revealed_vec.len() {
        revealed_messages.insert(
            *revealed_vec[i],
            SignatureMessage::hash(messages[i].clone()),
        );
    }

    let signature_proof = SignatureProof {
        revealed_messages,
        proof,
    };

    Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
        verified: Verifier::verify_signature_pok(&proof_request, &signature_proof, &nonce).is_ok(),
        error: None,
    })
    .unwrap())
}

fn bls_generate_keypair<G: CurveProjective<Engine = Bls12, Scalar = Fr> + SerDes>(
    seed: Option<Vec<u8>>,
) -> JsValue {
    let seed_data = match seed {
        Some(s) => s.to_vec(),
        None => {
            let mut rng = thread_rng();
            let mut s = vec![0u8, 32];
            rng.fill_bytes(s.as_mut_slice());
            s
        }
    };

    let sk = gen_sk(seed_data.as_slice());
    let mut pk = G::one();
    pk.mul_assign(sk);

    let mut pk_bytes = Vec::new();
    pk.serialize(&mut pk_bytes, true).unwrap();

    let keypair = BlsKeyPair {
        publicKey: Some(pk_bytes),
        secretKey: Some(SecretKey::from(sk)),
    };
    serde_wasm_bindgen::to_value(&keypair).unwrap()
}

fn gen_sk(msg: &[u8]) -> Fr {
    use sha2::digest::generic_array::{typenum::U48, GenericArray};
    const SALT: &[u8] = b"BLS-SIG-KEYGEN-SALT-";
    // copy of `msg` with appended zero byte
    let mut msg_prime = Vec::<u8>::with_capacity(msg.as_ref().len() + 1);
    msg_prime.extend_from_slice(msg.as_ref());
    msg_prime.extend_from_slice(&[0]);
    // `result` has enough length to hold the output from HKDF expansion
    let mut result = GenericArray::<u8, U48>::default();
    assert!(hkdf::Hkdf::<sha2::Sha256>::new(Some(SALT), &msg_prime[..])
        .expand(&[0, 48], &mut result)
        .is_ok());
    Fr::from_okm(&result)
}

/// Creates a BBS+ PoK from termwise-encoded multiple credentials
#[wasm_bindgen(js_name = blsCreateProofMulti)]
pub async fn bls_create_proof_multi(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let request: BlsCreateProofMultiRequest = request.try_into()?;
    let num_of_inputs = request.messages.len();

    if [
        request.signature.len(),
        request.revealed.len(),
        request.publicKey.len(),
    ]
    .iter()
    .any(|&x| x != num_of_inputs)
    {
        return Err(JsValue::from(
            "numbers of messages, signature, revealed, and publicKey must be the same",
        ));
    }

    let mut poks: Vec<PoKOfSignature> = Vec::with_capacity(num_of_inputs);
    let mut message_counts: Vec<usize> = Vec::with_capacity(num_of_inputs);
    let mut revealeds: Vec<Vec<usize>> = Vec::with_capacity(num_of_inputs);

    // generate blindings and hashmaps based on request.equivs
    let equiv_blindings: Vec<ProofNonce> = request
        .equivs
        .iter()
        .map(|_| ProofNonce::random())
        .collect();
    let mut equivs_map: HashMap<(usize, usize), usize> = HashMap::new();
    for (i, eq) in request.equivs.iter().enumerate() {
        for &e in eq {
            equivs_map.insert(e, i);
        }
    }

    // (1) commit
    for (i, (r_messages, r_signature, r_revealed, r_pk)) in multizip((
        request.messages,
        request.signature,
        request.revealed,
        request.publicKey,
    ))
    .enumerate()
    {
        if r_revealed.iter().any(|r| *r > r_messages.len()) {
            return Err(JsValue::from("revealed value is out of bounds"));
        }
        let pk = r_pk.to_public_key(r_messages.len())?;
        let revealed: BTreeSet<&usize> = r_revealed.iter().collect();
        let mut messages = Vec::new();
        for (j, r_message) in r_messages.iter().enumerate() {
            if revealed.contains(&j) {
                match equivs_map.get(&(i, j)) {
                    None => {
                        messages.push(ProofMessage::Revealed(SignatureMessage::hash(&r_message)))
                    }
                    Some(&k) => {
                        messages.push(ProofMessage::Hidden(HiddenMessage::ExternalBlinding(
                            SignatureMessage::hash(&r_message),
                            equiv_blindings[k],
                        )))
                    }
                }
            } else {
                messages.push(ProofMessage::Hidden(HiddenMessage::ProofSpecificBlinding(
                    SignatureMessage::hash(&r_message),
                )));
            }
        }
        match PoKOfSignature::init(&r_signature, &pk, messages.as_slice()) {
            Err(e) => return Err(JsValue::from(&format!("{:?}", e))),
            Ok(pok) => {
                poks.push(pok);
                message_counts.push(r_messages.len());
                revealeds.push(r_revealed);
            }
        }
    }

    // (2) challenge
    let mut challenge_bytes = Vec::new();
    for pok in &poks {
        challenge_bytes.extend_from_slice(pok.to_bytes().as_slice());
    }
    if request.nonce.is_empty() {
        challenge_bytes.extend_from_slice(&[0u8; FR_COMPRESSED_SIZE]);
    } else {
        let nonce = ProofNonce::hash(&request.nonce);
        challenge_bytes.extend_from_slice(nonce.to_bytes_uncompressed_form().as_ref());
    }
    let challenge_hash = ProofChallenge::hash(&challenge_bytes);

    // (3) response
    let mut proofs: Vec<PoKOfSignatureProofMultiWrapper> = Vec::with_capacity(num_of_inputs);
    for (pok, revealed) in multizip((poks, revealeds)) {
        match pok.gen_proof(&challenge_hash) {
            Ok(proof) => {
                let out = PoKOfSignatureProofMultiWrapper { revealed, proof };
                proofs.push(out);
            }
            Err(e) => return Err(JsValue::from(&format!("{:?}", e))),
        }
    }

    Ok(serde_wasm_bindgen::to_value(&proofs).unwrap())
}

/// Verify a BBS+ PoK from termwise-encoded multiple credentials
#[wasm_bindgen(js_name = blsVerifyProofMulti)]
pub async fn bls_verify_proof_multi(request: JsValue) -> Result<JsValue, JsValue> {
    set_panic_hook();
    let res = serde_wasm_bindgen::from_value::<BlsVerifyProofMultiContext>(request);
    let request: BlsVerifyProofMultiContext;
    match res {
        Ok(r) => request = r,
        Err(e) => {
            return Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
                verified: false,
                error: Some(format!("{:?}", e)),
            })
            .unwrap())
        }
    };

    let num_of_inputs = request.messages.len();
    if [request.proof.len(), request.publicKey.len()]
        .iter()
        .any(|&x| x != num_of_inputs)
    {
        return Err(JsValue::from(
            "numbers of messages, proof, and publicKey must be the same",
        ));
    }

    let nonce = if request.nonce.is_empty() {
        ProofNonce::default()
    } else {
        ProofNonce::hash(&request.nonce)
    };

    // (1) generate challenge hash

    // (2) verify
    for (i, (r_messages, r_proof, r_pk)) in
        multizip((request.messages, request.proof, request.publicKey)).enumerate()
    {
        let pk = r_pk.to_public_key(message_count)?;
        let messages = request.messages.clone();
        let (revealed, proof) = request.proof.unwrap();
        let proof_request = ProofRequest {
            revealed_messages: revealed,
            verification_key: pk,
        };

        let revealed_vec = proof_request
            .revealed_messages
            .iter()
            .collect::<Vec<&usize>>();
        let mut revealed_messages = BTreeMap::new();
        for i in 0..revealed_vec.len() {
            revealed_messages.insert(
                *revealed_vec[i],
                SignatureMessage::hash(messages[i].clone()),
            );
        }

        let signature_proof = SignatureProof {
            revealed_messages,
            proof,
        };

        Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
            verified: Verifier::verify_signature_pok(&proof_request, &signature_proof, &nonce)
                .is_ok(),
            error: None,
        })
        .unwrap());
    }

    Ok(serde_wasm_bindgen::to_value(&BbsVerifyResponse {
        verified: false,
        error: None,
    })
    .unwrap())
}
