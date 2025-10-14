use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::{
        entity::Entity,
        lifecycle::Remove,
        message::MessageReader,
        observer::On,
        schedule::IntoScheduleConfigs,
        system::{Commands, Query},
    },
    log::debug,
    prelude::{Local, Res, ResMut},
    state::state::NextState,
    time::Time,
};
use bevy_quinnet::{
    server::{QuinnetServer, QuinnetServerPlugin},
    shared::QuinnetSyncPreUpdate,
};
use bevy_replicon::{
    prelude::{ClientStats, ConnectedClient, DisconnectRequest, ServerMessages, ServerState},
    server::ServerSystems,
    shared::backend::connected_client::{NetworkId, NetworkIdMap},
};

use crate::BYTES_PER_SEC_PERIOD;

pub struct RepliconQuinnetServerPlugin;

impl Plugin for RepliconQuinnetServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(QuinnetServerPlugin::default())
            .configure_sets(
                PreUpdate,
                ServerSystems::ReceivePackets.after(QuinnetSyncPreUpdate),
            )
            .add_observer(disconnect_client)
            .add_systems(
                PreUpdate,
                (
                    set_running.run_if(bevy_quinnet::server::server_just_opened),
                    set_stopped.run_if(bevy_quinnet::server::server_just_closed),
                    (receive_packets, update_statistics, process_server_events)
                        .run_if(bevy_quinnet::server::server_listening),
                )
                    .in_set(ServerSystems::ReceivePackets),
            )
            .add_systems(
                PostUpdate,
                (
                    send_packets
                        .in_set(ServerSystems::SendPackets)
                        .run_if(bevy_quinnet::server::server_listening),
                    disconnect_by_request.after(ServerSystems::SendPackets),
                ),
            );
    }
}

fn set_running(mut state: ResMut<NextState<ServerState>>) {
    state.set(ServerState::Running);
}

fn set_stopped(mut state: ResMut<NextState<ServerState>>) {
    state.set(ServerState::Stopped);
}

fn process_server_events(
    mut commands: Commands,
    mut conn_events: MessageReader<bevy_quinnet::server::ConnectionEvent>,
    mut conn_lost_events: MessageReader<bevy_quinnet::server::ConnectionLostEvent>,
    network_map: Res<NetworkIdMap>,
) {
    for event in conn_events.read() {
        let network_id = NetworkId::new(event.id);
        const DEFAULT_INITIAL_MAX_DATAGRAM_SIZE: usize = 1200;
        commands.spawn((
            ConnectedClient {
                max_size: DEFAULT_INITIAL_MAX_DATAGRAM_SIZE,
            },
            network_id,
        ));
    }
    for event in conn_lost_events.read() {
        let network_id = NetworkId::new(event.id);
        if let Some(&client_entity) = network_map.get(&network_id) {
            // Entity could have been despawned by user.
            commands.entity(client_entity).despawn();
        }
    }
}

fn update_statistics(
    mut bps_timer: Local<f64>,
    mut clients: Query<(&NetworkId, &mut ConnectedClient, &mut ClientStats)>,
    mut quinnet_server: ResMut<QuinnetServer>,
    time: Res<Time>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (network_id, mut client, mut client_stats) in clients.iter_mut() {
        let Some(con) = endpoint.connection_mut(network_id.get()) else {
            return;
        };

        if let Some(max_size) = con.max_datagram_size() {
            client.max_size = max_size;
        }

        let quinn_stats = con.quinn_connection_stats();

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
}

fn receive_packets(
    mut quinnet_server: ResMut<QuinnetServer>,
    mut messages: ResMut<ServerMessages>,
    mut clients: Query<(Entity, &NetworkId)>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (client_entity, network_id) in &mut clients {
        let Some(con) = endpoint.connection_mut(network_id.get()) else {
            continue;
        };
        while let Ok((channel_id, message)) = con.dequeue_undispatched_bytes_from_peer() {
            messages.insert_received(client_entity, channel_id, message);
        }
    }
}

fn send_packets(
    mut quinnet_server: ResMut<QuinnetServer>,
    mut messages: ResMut<ServerMessages>,
    clients: Query<&NetworkId>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (client_entity, channel_id, message) in messages.drain_sent() {
        let network_id = clients
            .get(client_entity)
            .expect("messages should be sent only to connected clients");
        endpoint.try_send_payload_on(network_id.get(), channel_id as u8, message);
    }
}

fn disconnect_by_request(
    mut commands: Commands,
    mut disconnect_events: MessageReader<DisconnectRequest>,
) {
    for event in disconnect_events.read() {
        debug!("despawning client `{}` by disconnect request", event.client);
        commands.entity(event.client).despawn();
    }
}

fn disconnect_client(
    remove: On<Remove, ConnectedClient>,
    mut quinnet_server: ResMut<QuinnetServer>,
    clients: Query<&NetworkId>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };

    debug!("disconnecting despawned client `{}`", remove.entity);

    let network_id = clients.get(remove.entity).expect("inserted on connection");
    endpoint.try_disconnect_client(network_id.get());
}
