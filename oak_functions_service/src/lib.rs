//
// Copyright 2022 The Project Oak Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

#![no_std]
#![feature(never_type)]

extern crate alloc;

pub mod schema {
    #![allow(dead_code)]
    use prost::Message;
    include!(concat!(env!("OUT_DIR"), "/oak.functions.rs"));
}
mod logger;
mod remote_attestation;
mod wasm;

use crate::remote_attestation::{AttestationHandler, AttestationSessionHandler};
use alloc::{boxed::Box, format, sync::Arc};
use oak_functions_abi::Request;
use oak_functions_lookup::LookupDataManager;
use oak_functions_wasm::WasmHandler;
use oak_remote_attestation_interactive::handshaker::AttestationGenerator;
use remote_attestation::Handler;
use schema::LookupDataChunk;

pub use crate::logger::StandaloneLogger;

enum InitializationState {
    Uninitialized,
    Initialized(Box<dyn AttestationHandler>),
}

pub struct OakFunctionsService {
    attestation_generator: Arc<dyn AttestationGenerator>,
    initialization_state: InitializationState,
    lookup_data_manager: Arc<LookupDataManager<logger::StandaloneLogger>>,
}

impl OakFunctionsService {
    pub fn new(attestation_generator: Arc<dyn AttestationGenerator>) -> Self {
        Self {
            attestation_generator,
            initialization_state: InitializationState::Uninitialized,
            lookup_data_manager: Arc::new(
                LookupDataManager::new_empty(StandaloneLogger::default()),
            ),
        }
    }
}

impl<L: oak_logger::OakLogger> Handler for WasmHandler<L> {
    fn handle(&mut self, request: &[u8]) -> anyhow::Result<micro_rpc::Vec<u8>> {
        let request = Request {
            body: request.to_vec(),
        };
        let response = self.handle_invoke(request)?;
        Ok(response.body)
    }
}

impl schema::OakFunctions for OakFunctionsService {
    fn initialize(
        &mut self,
        initialization: &schema::InitializeRequest,
    ) -> Result<schema::InitializeResponse, micro_rpc::Status> {
        match &mut self.initialization_state {
            InitializationState::Initialized(_attestation_handler) => {
                Err(micro_rpc::Status::new_with_message(
                    micro_rpc::StatusCode::FailedPrecondition,
                    "already initialized",
                ))
            }
            InitializationState::Uninitialized => {
                // TODO(#3442): Implement constant response size policy.
                let wasm_handler = wasm::new_wasm_handler(
                    &initialization.wasm_module,
                    self.lookup_data_manager.clone(),
                )
                .map_err(|err| {
                    micro_rpc::Status::new_with_message(
                        micro_rpc::StatusCode::Internal,
                        format!("couldn't initialize Wasm handler: {:?}", err),
                    )
                })?;
                let attestation_handler = Box::new(
                    AttestationSessionHandler::create(
                        self.attestation_generator.clone(),
                        wasm_handler,
                    )
                    .map_err(|err| {
                        micro_rpc::Status::new_with_message(
                            micro_rpc::StatusCode::Internal,
                            format!("couldn't create attestation handler: {:?}", err),
                        )
                    })?,
                );
                let public_key_info = attestation_handler.get_public_key_info();
                self.initialization_state = InitializationState::Initialized(attestation_handler);
                Ok(schema::InitializeResponse {
                    public_key_info: Some(schema::PublicKeyInfo {
                        public_key: public_key_info.public_key,
                        attestation: public_key_info.attestation,
                    }),
                })
            }
        }
    }

    fn invoke(
        &mut self,
        request_message: &schema::InvokeRequest,
    ) -> Result<schema::InvokeResponse, micro_rpc::Status> {
        match &mut self.initialization_state {
            InitializationState::Uninitialized => Err(micro_rpc::Status::new_with_message(
                micro_rpc::StatusCode::FailedPrecondition,
                "not initialized",
            )),
            InitializationState::Initialized(attestation_handler) => {
                let response =
                    attestation_handler
                        .message(&request_message.body)
                        .map_err(|err| {
                            micro_rpc::Status::new_with_message(
                                micro_rpc::StatusCode::Internal,
                                format!("{:?}", err),
                            )
                        })?;
                Ok(schema::InvokeResponse { body: response })
            }
        }
    }

    fn extend_next_lookup_data(
        &mut self,
        request: &schema::ExtendNextLookupDataRequest,
    ) -> Result<schema::ExtendNextLookupDataResponse, micro_rpc::Status> {
        self.lookup_data_manager
            .extend_next_lookup_data(to_data(&request.chunk));
        Ok(schema::ExtendNextLookupDataResponse {})
    }

    fn finish_next_lookup_data(
        &mut self,
        _request: &schema::FinishNextLookupDataRequest,
    ) -> Result<schema::FinishNextLookupDataResponse, micro_rpc::Status> {
        self.lookup_data_manager.finish_next_lookup_data();
        Ok(schema::FinishNextLookupDataResponse {})
    }

    fn abort_next_lookup_data(
        &mut self,
        _request: &schema::Empty,
    ) -> Result<schema::AbortNextLookupDataResponse, micro_rpc::Status> {
        self.lookup_data_manager.abort_next_lookup_data();
        Ok(schema::AbortNextLookupDataResponse {})
    }
}

// Helper function to convert LookupDataChunk to Data.
// TODO(#3791): Check if we really have to copy here.
fn to_data(chunk: &Option<LookupDataChunk>) -> oak_functions_lookup::Data {
    chunk
        .as_ref()
        .unwrap()
        .items
        .iter()
        .map(|entry| (entry.key.clone(), entry.value.clone()))
        .collect()
}
