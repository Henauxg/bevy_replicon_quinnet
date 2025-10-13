use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::schedule::IntoScheduleConfigs,
    prelude::{Local, Res, ResMut},
    state::state::NextState,
    time::Time,
};
use bevy_quinnet::{
    client::{QuinnetClient, QuinnetClientPlugin},
    shared::QuinnetSyncPreUpdate,
};
use bevy_replicon::{
    client::ClientSystems,
    prelude::{ClientMessages, ClientState, ClientStats},
};

use crate::BYTES_PER_SEC_PERIOD;

pub struct RepliconQuinnetClientPlugin;

impl Plugin for RepliconQuinnetClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(QuinnetClientPlugin::default())
            .configure_sets(
                PreUpdate,
                ClientSystems::ReceivePackets.after(QuinnetSyncPreUpdate),
            )
            .add_systems(
                PreUpdate,
                (
                    set_connected.run_if(bevy_quinnet::client::client_just_connected),
                    set_connecting.run_if(bevy_quinnet::client::client_connecting),
                    set_disconnected.run_if(bevy_quinnet::client::client_just_disconnected),
                    (receive_packets, update_statistics)
                        .run_if(bevy_quinnet::client::client_connected),
                )
                    .in_set(ClientSystems::ReceivePackets),
            )
            .add_systems(
                PostUpdate,
                send_packets
                    .in_set(ClientSystems::SendPackets)
                    .run_if(bevy_quinnet::client::client_connected),
            );
    }
}

fn set_disconnected(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Disconnected);
}

fn set_connecting(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Connecting);
}

fn set_connected(mut state: ResMut<NextState<ClientState>>) {
    state.set(ClientState::Connected);
}

fn update_statistics(
    mut bps_timer: Local<f64>,
    mut quinnet_client: ResMut<QuinnetClient>,
    mut client_stats: ResMut<ClientStats>,
    time: Res<Time>,
) {
    let Some(con) = quinnet_client.get_connection_mut() else {
        return;
    };
    let Some(quinn_stats) = con.quinn_connection_stats() else {
        return;
    };

    client_stats.rtt = quinn_stats.path.rtt.as_secs_f64();
    client_stats.packet_loss =
        100. * (quinn_stats.path.lost_packets as f64 / quinn_stats.path.sent_packets as f64);

    *bps_timer += time.delta_secs_f64();
    if *bps_timer >= BYTES_PER_SEC_PERIOD {
        *bps_timer = 0.;
        let stats = con.stats_mut();
        let received_bytes_count = stats.clear_received_bytes_count() as f64;
        let sent_bytes_count = stats.clear_sent_bytes_count() as f64;
        client_stats.received_bps = received_bytes_count / BYTES_PER_SEC_PERIOD;
        client_stats.sent_bps = sent_bytes_count / BYTES_PER_SEC_PERIOD;
    }
}

fn receive_packets(
    mut quinnet_client: ResMut<QuinnetClient>,
    mut messages: ResMut<ClientMessages>,
) {
    let Some(connection) = quinnet_client.get_connection_mut() else {
        return;
    };

    while let Ok((channel_id, message)) = connection.dequeue_undispatched_bytes_from_peer() {
        messages.insert_received(channel_id, message);
    }
}

fn send_packets(mut quinnet_client: ResMut<QuinnetClient>, mut messages: ResMut<ClientMessages>) {
    let Some(connection) = quinnet_client.get_connection_mut() else {
        return;
    };
    for (channel_id, message) in messages.drain_sent() {
        connection.try_send_payload_on(channel_id as u8, message);
    }
}
