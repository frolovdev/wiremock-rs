use crate::{
    active_mock::ActiveMock,
    verification::{VerificationOutcome, VerificationReport},
};
use crate::{Mock, Request, ResponseTemplate};
use futures_timer::Delay;
use http_types::{Response, StatusCode};
use log::debug;
use std::ops::{Index, IndexMut};

/// The collection of mocks used by a `MockServer` instance to match against
/// incoming requests.
///
/// New mocks are added to `ActiveMockSet` every time [`MockServer::register`](crate::MockServer::register),
/// [`MockServer::register_scoped`](crate::MockServer::register_scoped) or
/// [`Mock::mount`](crate::Mock::mount) are called.
pub(crate) struct ActiveMockSet {
    mocks: Vec<ActiveMock>,
    /// A counter that keeps track of how many times [`ActiveMockSet::reset`] has been called.
    /// It starts at `0` and gets incremented for each invocation.
    ///
    /// We need `generation` to know if a [`MockId`] points to an [`ActiveMock`] that has been
    /// removed via [`ActixMockSet::reset`].
    generation: u16,
}

/// A `MockId` is an opaque index that uniquely identifies an [`ActiveMock`] inside an [`ActiveMockSet`].  
///
/// The only way to create a `MockId` is calling [`ActiveMockSet::register`].
#[derive(Copy, Clone)]
pub(crate) struct MockId {
    index: usize,
    /// The generation of [`ActiveMockSet`] when [`ActiveMockSet::register`] was called.
    /// It allows [`ActiveMockSet`] to check that the [`ActiveMock`] our [`MockId`] points to is still in
    /// the set (i.e. the set has not been wiped by a [`ActiveMockSet::reset`] call).
    generation: u16,
}

impl ActiveMockSet {
    /// Create a new instance of MockSet.
    pub(crate) fn new() -> ActiveMockSet {
        ActiveMockSet {
            mocks: vec![],
            generation: 0,
        }
    }

    pub(crate) async fn handle_request(&mut self, request: Request) -> (Response, Option<Delay>) {
        debug!("Handling request.");
        let mut response_template: Option<ResponseTemplate> = None;
        for mock in &mut self.mocks {
            if mock.matches(&request) {
                response_template = Some(mock.response_template(&request));
                break;
            }
        }
        if let Some(response_template) = response_template {
            let delay = response_template.delay().map(|d| Delay::new(d.to_owned()));
            (response_template.generate_response(), delay)
        } else {
            debug!("Got unexpected request:\n{}", request);
            (Response::new(StatusCode::NotFound), None)
        }
    }

    pub(crate) fn register(&mut self, mock: Mock) -> MockId {
        let n_registered_mocks = self.mocks.len();
        let active_mock = ActiveMock::new(mock, n_registered_mocks);
        self.mocks.push(active_mock);

        MockId {
            index: self.mocks.len() - 1,
            generation: self.generation,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.mocks = vec![];
        self.generation += 1;
    }

    /// De-activate one of the mocks in the set. It will stop matching against incoming requests,
    /// regardless of its specification.
    pub(crate) fn deactivate(&mut self, mock_id: MockId) {
        let mock = &mut self[mock_id];
        mock.active = false;
    }

    /// Verify that expectations have been met for **all** [`ActiveMock`]s in the set.
    pub(crate) fn verify_all(&self) -> VerificationOutcome {
        let failed_verifications: Vec<VerificationReport> = self
            .mocks
            .iter()
            .map(ActiveMock::verify)
            .filter(|verification_report| !verification_report.is_satisfied())
            .collect();
        if failed_verifications.is_empty() {
            VerificationOutcome::Success
        } else {
            VerificationOutcome::Failure(failed_verifications)
        }
    }

    /// Verify that expectations have been met for the [`ActiveMock`] corresponding to the specified [`MockId`].
    pub(crate) fn verify(&self, mock_id: MockId) -> VerificationReport {
        let mock = &self[mock_id];
        mock.verify()
    }
}

impl IndexMut<MockId> for ActiveMockSet {
    fn index_mut(&mut self, index: MockId) -> &mut Self::Output {
        if index.generation != self.generation {
            panic!("The mock you are trying to access is no longer active. It has been deleted from the active set via `reset` - you should not hold on to a `MockId` after you call `reset`!.")
        }
        &mut self.mocks[index.index]
    }
}

impl Index<MockId> for ActiveMockSet {
    type Output = ActiveMock;

    fn index(&self, index: MockId) -> &Self::Output {
        if index.generation != self.generation {
            panic!("The mock you are trying to access is no longer active. It has been deleted from the active set via `reset` - you should not hold on to a `MockId` after you call `reset`!.")
        }
        &self.mocks[index.index]
    }
}

#[cfg(test)]
mod tests {
    use crate::mock_set::ActiveMockSet;

    #[test]
    fn generation_is_incremented_for_every_reset() {
        let mut set = ActiveMockSet::new();
        assert_eq!(set.generation, 0);

        for i in 1..10 {
            set.reset();
            assert_eq!(set.generation, i);
        }
    }
}
