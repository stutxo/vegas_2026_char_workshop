use crate::da::{DaError, RuntimeError, SemanticError};
use bitcoind_async_client::error::ClientError;

pub(crate) fn map_rpc_client_error(chain_name: &'static str, err: ClientError) -> DaError {
    if err.is_tx_not_found() {
        return SemanticError::NotFound.into();
    }

    match err {
        ClientError::Connection(message) => RuntimeError::ConnectionFailure(message).into(),
        ClientError::Timeout => RuntimeError::Timeout(format!("{chain_name} RPC timed out")).into(),
        ClientError::Status(_, message)
        | ClientError::HttpRedirect(message)
        | ClientError::Request(message) => RuntimeError::ServiceUnavailable(message).into(),
        ClientError::MaxRetriesExceeded(retries) => {
            RuntimeError::ServiceUnavailable(format!("max RPC retries exceeded: {retries}")).into()
        }
        ClientError::WrongNetworkAddress(network) => {
            RuntimeError::Misconfigured(format!("{chain_name} RPC is connected to {network:?}"))
                .into()
        }
        other => RuntimeError::Internal(other.to_string()).into(),
    }
}
