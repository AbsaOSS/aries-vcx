use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use did_doc::schema::did_doc::DidDocumentBuilder;
use did_parser_nom::Did;
use public_key::Key;

use crate::{
    error::DidPeerError,
    peer_did::numalgos::numalgo2::{
        purpose::ElementPurpose,
        service_abbreviation::{deabbreviate_service, ServiceAbbreviatedDidPeer2},
        verification_method::get_verification_methods_by_key,
    },
    resolver::options::PublicKeyEncoding,
};

pub fn didpeer_elements_to_diddoc(
    mut did_doc_builder: DidDocumentBuilder,
    did: &Did,
    public_key_encoding: PublicKeyEncoding,
) -> Result<DidDocumentBuilder, DidPeerError> {
    let mut service_index: usize = 0;

    // Skipping one here because the first element is empty string
    for element in did.id()[1..].split('.').skip(1) {
        did_doc_builder = process_element(
            element,
            did_doc_builder,
            &mut service_index,
            did,
            public_key_encoding,
        )?;
    }

    Ok(did_doc_builder)
}

fn process_element(
    element: &str,
    mut did_doc_builder: DidDocumentBuilder,
    service_index: &mut usize,
    did: &Did,
    public_key_encoding: PublicKeyEncoding,
) -> Result<DidDocumentBuilder, DidPeerError> {
    let purpose: ElementPurpose = element
        .chars()
        .next()
        .ok_or(DidPeerError::DidValidationError(format!(
            "No purpose code following element separator in '{}'",
            element
        )))?
        .try_into()?;
    let purposeless_element = &element[1..];

    if purpose == ElementPurpose::Service {
        did_doc_builder =
            process_service_element(purposeless_element, did_doc_builder, service_index)?;
    } else {
        did_doc_builder = process_key_element(
            purposeless_element,
            did_doc_builder,
            did,
            public_key_encoding,
            purpose,
        )?;
    }

    Ok(did_doc_builder)
}

fn process_service_element(
    element: &str,
    mut did_doc_builder: DidDocumentBuilder,
    service_index: &mut usize,
) -> Result<DidDocumentBuilder, DidPeerError> {
    let decoded = STANDARD_NO_PAD.decode(element)?;
    let service: ServiceAbbreviatedDidPeer2 = serde_json::from_slice(&decoded)?;

    did_doc_builder = did_doc_builder.add_service(deabbreviate_service(service, *service_index)?);
    *service_index += 1;

    Ok(did_doc_builder)
}

fn process_key_element(
    element: &str,
    mut did_doc_builder: DidDocumentBuilder,
    did: &Did,
    public_key_encoding: PublicKeyEncoding,
    purpose: ElementPurpose,
) -> Result<DidDocumentBuilder, DidPeerError> {
    let key = Key::from_fingerprint(element)?;
    let vms = get_verification_methods_by_key(&key, did, public_key_encoding)?;

    for vm in vms.into_iter() {
        match purpose {
            ElementPurpose::Assertion => {
                did_doc_builder = did_doc_builder.add_assertion_method(vm);
            }
            ElementPurpose::Encryption => {
                did_doc_builder = did_doc_builder.add_key_agreement(vm);
            }
            ElementPurpose::Verification => {
                did_doc_builder = did_doc_builder.add_verification_method(vm);
            }
            ElementPurpose::CapabilityInvocation => {
                did_doc_builder = did_doc_builder.add_capability_invocation(vm)
            }
            ElementPurpose::CapabilityDelegation => {
                did_doc_builder = did_doc_builder.add_capability_delegation(vm)
            }
            _ => return Err(DidPeerError::UnsupportedPurpose(purpose.into())),
        }
    }

    Ok(did_doc_builder)
}

#[cfg(test)]
mod tests {
    use did_doc::schema::service::typed::ServiceType;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_process_elements_empty_did() {
        let did: Did = "did:peer:2".parse().unwrap();

        let built_ddo = didpeer_elements_to_diddoc(
            DidDocumentBuilder::new(did.clone()),
            &did,
            PublicKeyEncoding::Base58,
        )
        .unwrap()
        .build();
        assert_eq!(built_ddo.id().to_string(), did.to_string());
    }

    #[test]
    fn test_process_elements_with_multiple_elements() {
        let did: Did =
            "did:peer:2.Vz6MkqRYqQiSgvZQdnBytw86Qbs2ZWUkGv22od935YF4s8M7V.\
             SeyJpZCI6IiNzZXJ2aWNlLTAiLCJ0IjoiZG0iLCJzIjoiaHR0cHM6Ly9leGFtcGxlLmNvbS9lbmRwb2ludCJ9"
                .parse()
                .unwrap();

        let processed_did_doc_builder = didpeer_elements_to_diddoc(
            DidDocumentBuilder::new(did.clone()),
            &did,
            PublicKeyEncoding::Multibase,
        )
        .unwrap();
        let built_ddo = processed_did_doc_builder.build();

        assert_eq!(built_ddo.id().to_string(), did.to_string());
        assert_eq!(built_ddo.verification_method().len(), 1);
        assert_eq!(built_ddo.service().len(), 1);
    }

    #[test]
    fn test_process_elements_error_on_invalid_element() {
        let did: Did =
            "did:peer:2.Vz6MkqRYqQiSgvZQdnBytw86Qbs2ZWUkGv22od935YF4s8M7V.\
             SeyJpZCI6IiNzZXJ2aWNlLTAiLCJ0IjoiZG0iLCJzIjoiaHR0cHM6Ly9leGFtcGxlLmNvbS9lbmRwb2ludCJ9.\
             Xinvalid"
                .parse()
                .unwrap();

        match didpeer_elements_to_diddoc(
            DidDocumentBuilder::new(did.clone()),
            &did,
            PublicKeyEncoding::Multibase,
        ) {
            Ok(_) => panic!("Expected Err, got Ok"),
            Err(e) => {
                assert!(matches!(e, DidPeerError::UnsupportedPurpose('X')));
            }
        }
    }

    #[test]
    fn test_process_service_element_one_service() {
        let purposeless_service_element =
            "eyJ0IjoiZG0iLCJzIjoiaHR0cHM6Ly9leGFtcGxlLmNvbS9lbmRwb2ludCJ9";
        let did: Did = format!("did:peer:2.S{}", purposeless_service_element)
            .parse()
            .unwrap();
        let mut index = 0;
        let ddo_builder = DidDocumentBuilder::new(did);
        let built_ddo =
            process_service_element(purposeless_service_element, ddo_builder, &mut index)
                .unwrap()
                .build();
        assert_eq!(built_ddo.service().len(), 1);
        let service = built_ddo.service().first().unwrap();
        assert_eq!(service.id().to_string(), "#service-0".to_string());
        assert_eq!(service.service_types(), vec!(ServiceType::DIDCommV2));
        assert_eq!(
            service.service_endpoint().to_string(),
            "https://example.com/endpoint".to_string()
        );
    }

    #[test]
    fn test_process_key_element() {
        let purposeless_key_element = "z6MkqRYqQiSgvZQdnBytw86Qbs2ZWUkGv22od935YF4s8M7V";
        let did: Did = format!("did:peer:2.V{}", purposeless_key_element)
            .parse()
            .unwrap();

        let ddo_builder = DidDocumentBuilder::new(did.clone());
        let public_key_encoding = PublicKeyEncoding::Multibase;
        let built_ddo = process_key_element(
            purposeless_key_element,
            ddo_builder,
            &did,
            public_key_encoding,
            ElementPurpose::Verification,
        )
        .unwrap()
        .build();

        assert_eq!(built_ddo.verification_method().len(), 1);
        let vm = built_ddo.verification_method().first().unwrap();
        assert_eq!(vm.id().to_string(), "#6MkqRYqQ");
        assert_eq!(vm.controller().to_string(), did.to_string());
    }

    #[test]
    fn test_process_key_element_negative() {
        let did: Did = "did:peer:2".parse().unwrap();
        assert!(process_key_element(
            "z6MkqRYqQiSgvZQdnBytw86Qbs2ZWUkGv22od935YF4s8M7V",
            DidDocumentBuilder::new(did.clone()),
            &did,
            PublicKeyEncoding::Multibase,
            ElementPurpose::Service
        )
        .is_err());
    }
}
