use crate::{
    message::{tab::TabRecv, tab_manager::TabManagerRecv},
    state::tab::TabsState,
};
use crate::{
    message::{tab::TabSend, tab_assignment::AssignTab},
    prelude::*,
};
use anyhow::Context;
use postage::sink::Sink;

use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tab_api::tab::{TabId, TabMetadata};

/// Manages the currently running tabs.  This is a point-of-contact between the tab-command and tab-pty clients.
///
/// - Serves 'create tab' requests from the tab-command client.
/// - Spawns tab-pty processes (OS processes), and issues offers of tab assignment to connected pty clients.
/// - Terminates tabs when requested by the tab-command client.
pub struct TabManagerService {
    _recv: Lifeline,
}

static TAB_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
// working on a bug here where all the ptys disconnect, and TabRecv goes dead.
impl Service for TabManagerService {
    type Bus = ListenerBus;
    type Lifeline = anyhow::Result<Self>;

    fn spawn(bus: &Self::Bus) -> Self::Lifeline {
        let _recv = {
            let mut rx = bus.rx::<TabManagerRecv>()?;

            let mut tx = bus.tx::<TabSend>()?;
            let mut tx_tabs = bus.tx::<TabRecv>()?;
            let mut tx_tabs_state = bus.tx::<TabsState>()?.log(Level::Debug);
            let mut tx_assign_tab = bus.tx::<AssignTab>()?;

            let mut tabs: HashMap<TabId, TabMetadata> = HashMap::new();

            Self::try_task("recv", async move {
                'msg: while let Some(msg) = rx.recv().await {
                    match msg {
                        TabManagerRecv::CreateTab(create) => {
                            for tab in tabs.values() {
                                if tab.name == create.name {
                                    continue 'msg;
                                }
                            }

                            debug!("recieved request to create tab {}", &create.name);
                            let id = TAB_ID_COUNTER.fetch_add(1, Ordering::SeqCst) as u16;
                            let tab_id = TabId(id);
                            let tab_metadata = TabMetadata::create(tab_id, create);

                            tx_assign_tab.send(AssignTab(tab_metadata.clone())).await?;

                            tabs.insert(tab_id, tab_metadata);
                            tx_tabs_state.send(TabsState::new(&tabs)).await?;
                        }
                        TabManagerRecv::UpdateTimestamp(id) => {
                            if let Some(metadata) = tabs.get_mut(&id) {
                                metadata.mark_selected();

                                tx.send(TabSend::Updated(metadata.clone())).await?;
                            }
                        }
                        TabManagerRecv::CloseTab(close) => {
                            Self::close_tab(
                                close,
                                &mut tabs,
                                &mut tx,
                                &mut tx_tabs,
                                &mut tx_tabs_state,
                            )
                            .await?;
                        }
                    }
                }
                Ok(())
            })
        };

        Ok(Self { _recv })
    }
}

impl TabManagerService {
    async fn close_tab(
        id: TabId,
        tabs: &mut HashMap<TabId, TabMetadata>,
        mut tx: impl Sink<Item = TabSend> + Unpin,
        mut tx_close: impl Sink<Item = TabRecv> + Unpin,
        mut tx_tabs_state: impl Sink<Item = TabsState> + Unpin,
    ) -> anyhow::Result<()> {
        info!("TabManager terminating tab {}", id);
        tabs.remove(&id);

        tx.send(TabSend::Stopped(id))
            .await
            .context("tx TabTerminated")
            .ok();
        tx_close.send(TabRecv::Terminate(id)).await.ok();
        tx_tabs_state
            .send(TabsState::new(&tabs))
            .await
            .context("tx_tabs_state TabsState")
            .ok();

        Ok(())
    }
}
