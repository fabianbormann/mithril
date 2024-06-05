use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use mithril_common::{
    entities::{
        BlockNumber, CardanoTransactionsSnapshot, Certificate, ProtocolMessagePartKey,
        SignedEntityType,
    },
    StdResult,
};

use crate::services::ProverService;

use super::ArtifactBuilder;

/// A [CardanoTransactionsArtifact] builder
pub struct CardanoTransactionsArtifactBuilder {
    prover_service: Arc<dyn ProverService>,
}

impl CardanoTransactionsArtifactBuilder {
    /// CardanoTransactions artifact builder factory
    pub fn new(prover_service: Arc<dyn ProverService>) -> Self {
        Self { prover_service }
    }
}

#[async_trait]
impl ArtifactBuilder<BlockNumber, CardanoTransactionsSnapshot>
    for CardanoTransactionsArtifactBuilder
{
    async fn compute_artifact(
        &self,
        beacon: BlockNumber,
        certificate: &Certificate,
    ) -> StdResult<CardanoTransactionsSnapshot> {
        let merkle_root = certificate
            .protocol_message
            .get_message_part(&ProtocolMessagePartKey::CardanoTransactionsMerkleRoot)
            .ok_or(anyhow!(
                "Can not find CardanoTransactionsMerkleRoot protocol message part in certificate"
            ))
            .with_context(|| {
                format!(
                    "Can not compute CardanoTransactionsSnapshot artifact for signed_entity: {:?}",
                    SignedEntityType::CardanoTransactions(certificate.epoch, beacon)
                )
            })?;
        self.prover_service.compute_cache(beacon).await?;

        Ok(CardanoTransactionsSnapshot::new(
            merkle_root.to_string(),
            beacon,
        ))
    }
}

#[cfg(test)]
mod tests {
    use mithril_common::{entities::ProtocolMessage, test_utils::fake_data};

    use crate::services::MockProverService;

    use super::*;

    #[tokio::test]
    async fn should_compute_valid_artifact_with_merkleroot() {
        let certificate = {
            let mut certificate = fake_data::certificate("certificate-123".to_string());
            let mut message = ProtocolMessage::new();
            message.set_message_part(
                ProtocolMessagePartKey::CardanoTransactionsMerkleRoot,
                "merkleroot".to_string(),
            );
            certificate.protocol_message = message;
            certificate
        };

        let beacon = 100;
        let mut mock_prover = MockProverService::new();
        mock_prover.expect_compute_cache().returning(|_| Ok(()));
        let cardano_transaction_artifact_builder =
            CardanoTransactionsArtifactBuilder::new(Arc::new(mock_prover));
        let artifact = cardano_transaction_artifact_builder
            .compute_artifact(beacon, &certificate)
            .await
            .unwrap();
        let artifact_expected = CardanoTransactionsSnapshot::new("merkleroot".to_string(), beacon);
        assert_eq!(artifact_expected, artifact);
    }

    #[tokio::test]
    async fn should_fail_to_compute_artifact_without_merkle_root() {
        let certificate = {
            let mut certificate = fake_data::certificate("certificate-123".to_string());
            let message = ProtocolMessage::new();
            certificate.protocol_message = message;
            certificate
        };

        let mut mock_prover = MockProverService::new();
        mock_prover.expect_compute_cache().returning(|_| Ok(()));
        let cardano_transaction_artifact_builder =
            CardanoTransactionsArtifactBuilder::new(Arc::new(mock_prover));
        cardano_transaction_artifact_builder
            .compute_artifact(12390, &certificate)
            .await
            .expect_err("The artifact building must fail since there is no CardanoTransactionsMerkleRoot part in its message.");
    }
}
