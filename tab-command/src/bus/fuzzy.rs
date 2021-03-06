use postage::{broadcast, mpsc, watch};

use crate::{
    message::fuzzy::FuzzyEvent,
    message::fuzzy::FuzzySelection,
    message::fuzzy::FuzzyShutdown,
    message::terminal::TerminalSend,
    message::terminal::TerminalShutdown,
    prelude::*,
    state::fuzzy::FuzzyMatchState,
    state::fuzzy::FuzzyOutputEvent,
    state::fuzzy::FuzzyQueryState,
    state::fuzzy::FuzzySelectState,
    state::fuzzy::FuzzyTabsState,
    state::{fuzzy::FuzzyEscapeState, workspace::WorkspaceState},
};

lifeline_bus!(pub struct FuzzyBus);

impl Message<FuzzyBus> for Option<FuzzyTabsState> {
    type Channel = watch::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzyQueryState {
    type Channel = watch::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzyMatchState {
    type Channel = watch::Sender<Self>;
}

impl Message<FuzzyBus> for Option<FuzzySelectState> {
    type Channel = watch::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzyEvent {
    type Channel = broadcast::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzySelection {
    type Channel = mpsc::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzyOutputEvent {
    type Channel = mpsc::Sender<Self>;
}

impl Message<FuzzyBus> for FuzzyShutdown {
    type Channel = mpsc::Sender<Self>;
}

impl Resource<FuzzyBus> for FuzzyEscapeState {}

pub struct TerminalFuzzyCarrier {
    _recv: Lifeline,
    _selection: Lifeline,
    _forward_shutdown: Lifeline,
}

impl CarryFrom<TerminalBus> for FuzzyBus {
    type Lifeline = anyhow::Result<TerminalFuzzyCarrier>;

    fn carry_from(&self, from: &TerminalBus) -> Self::Lifeline {
        let _recv = {
            let mut rx = from.rx::<Option<WorkspaceState>>()?.log(Level::Debug);
            let mut tx = self.tx::<Option<FuzzyTabsState>>()?;

            Self::task("recv", async move {
                while let Some(msg) = rx.recv().await {
                    tx.send(msg.map(WorkspaceState::into)).await.ok();
                }
            })
        };

        let _selection = {
            let mut rx = self.rx::<FuzzySelection>()?;
            let mut tx = from.tx::<TerminalSend>()?;

            Self::task("recv", async move {
                while let Some(msg) = rx.recv().await {
                    tx.send(TerminalSend::FuzzySelection(msg.0)).await.ok();
                }
            })
        };

        let _forward_shutdown = {
            let mut rx = self.rx::<FuzzyShutdown>()?;
            let mut tx = from.tx::<TerminalShutdown>()?;

            Self::task("recv", async move {
                if let Some(_shutdown) = rx.recv().await {
                    tx.send(TerminalShutdown {}).await.ok();
                }
            })
        };

        Ok(TerminalFuzzyCarrier {
            _recv,
            _selection,
            _forward_shutdown,
        })
    }
}
