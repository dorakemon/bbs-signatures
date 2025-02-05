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

import { BlsKeyPair } from "./BlsKeyPair";

/**
 * A request to create a BBS signature for a set of messages from a BLS12-381 key pair
 */
export interface BlindBlsSignContextRequest {
  /**
   * BLS12-381 key pair
   */
  readonly keyPair: BlsKeyPair;
  /**
   * Messages to sign
   */
  readonly messages: readonly Uint8Array[];
  /**
   * The resulting commitment of the blinded prover secret key to sign
   */
  readonly commitment: Uint8Array;
}
