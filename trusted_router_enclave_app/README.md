# Trusted Router

Enclave application that can be run under Restricted Kernel.

Trusted Router binary receives a microRPC request containing a packet and
routes it according to defined rules.

This binary uses microRPC to communicate between the Untrusted Launcher and the
guest VM. The interface is defined in
[`trusted_router.proto`](trusted_router_service/proto/trusted_router.proto).
