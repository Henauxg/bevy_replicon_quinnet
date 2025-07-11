use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    ecs::{
        entity::Entity,
        observer::Trigger,
        schedule::IntoScheduleConfigs,
        system::{Commands, Query},
        world::OnRemove,
    },
    log::debug,
    prelude::{EventReader, Local, Res, ResMut},
    time::Time,
};
use bevy_quinnet::{
    server::{QuinnetServer, QuinnetServerPlugin},
    shared::QuinnetSyncUpdate,
};
use bevy_replicon::{
    prelude::{ConnectedClient, DisconnectRequest, NetworkStats, RepliconServer},
    server::ServerSet,
    shared::backend::connected_client::{NetworkId, NetworkIdMap},
};

use crate::BYTES_PER_SEC_PERIOD;

pub struct RepliconQuinnetServerPlugin;

impl Plugin for RepliconQuinnetServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(QuinnetServerPlugin::default())
            .configure_sets(
                PreUpdate,
                ServerSet::ReceivePackets.after(QuinnetSyncUpdate),
            )
            .add_observer(disconnect_client)
            .add_systems(
                PreUpdate,
                (
                    set_running.run_if(bevy_quinnet::server::server_just_opened),
                    (receive_packets, update_statistics, process_server_events)
                        .run_if(bevy_quinnet::server::server_listening),
                )
                    .chain()
                    .in_set(ServerSet::ReceivePackets),
            )
            .add_systems(
                PostUpdate,
                (
                    set_stopped
                        .before(ServerSet::Send)
                        .run_if(bevy_quinnet::server::server_just_closed),
                    send_packets
                        .in_set(ServerSet::SendPackets)
                        .run_if(bevy_quinnet::server::server_listening),
                    disconnect_by_request.after(ServerSet::SendPackets),
                ),
            );
    }
}

fn set_running(mut server: ResMut<RepliconServer>) {
    server.set_running(true);
}

fn set_stopped(mut server: ResMut<RepliconServer>) {
    server.set_running(false);
}

fn process_server_events(
    mut commands: Commands,
    mut conn_events: EventReader<bevy_quinnet::server::ConnectionEvent>,
    mut conn_lost_events: EventReader<bevy_quinnet::server::ConnectionLostEvent>,
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
    mut clients: Query<(&NetworkId, &mut ConnectedClient, &mut NetworkStats)>,
    mut quinnet_server: ResMut<QuinnetServer>,
    time: Res<Time>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (network_id, mut client, mut client_stats) in clients.iter_mut() {
        let Some(con) = endpoint.get_connection_mut(network_id.get()) else {
            return;
        };

        if let Some(max_size) = con.max_datagram_size() {
            client.max_size = max_size;
        }

        let stats = con.connection_stats();

        client_stats.rtt = stats.path.rtt.as_secs_f64();
        client_stats.packet_loss =
            100. * (stats.path.lost_packets as f64 / stats.path.sent_packets as f64);

        *bps_timer += time.delta_secs_f64();
        if *bps_timer >= BYTES_PER_SEC_PERIOD {
            *bps_timer = 0.;
            let received_bytes_count = con.clear_received_bytes_count() as f64;
            let sent_bytes_count = con.clear_sent_bytes_count() as f64;
            client_stats.received_bps = received_bytes_count / BYTES_PER_SEC_PERIOD;
            client_stats.sent_bps = sent_bytes_count / BYTES_PER_SEC_PERIOD;
        }
    }
}

fn receive_packets(
    mut quinnet_server: ResMut<QuinnetServer>,
    mut replicon_server: ResMut<RepliconServer>,
    mut clients: Query<(Entity, &NetworkId)>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (client_entity, network_id) in &mut clients {
        while let Some((channel_id, message)) = endpoint.try_receive_payload_from(network_id.get())
        {
            replicon_server.insert_received(client_entity, channel_id, message);
        }
    }
}

fn send_packets(
    mut quinnet_server: ResMut<QuinnetServer>,
    mut replicon_server: ResMut<RepliconServer>,
    clients: Query<&NetworkId>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };
    for (client_entity, channel_id, message) in replicon_server.drain_sent() {
        let network_id = clients
            .get(client_entity)
            .expect("messages should be sent only to connected clients");
        endpoint.try_send_payload_on(network_id.get(), channel_id as u8, message);
    }
}

fn disconnect_by_request(
    mut commands: Commands,
    mut disconnect_events: EventReader<DisconnectRequest>,
) {
    for event in disconnect_events.read() {
        debug!(
            "despawning client `{}` by disconnect request",
            event.client_entity
        );
        commands.entity(event.client_entity).despawn();
    }
}

fn disconnect_client(
    trigger: Trigger<OnRemove, ConnectedClient>,
    mut quinnet_server: ResMut<QuinnetServer>,
    clients: Query<&NetworkId>,
) {
    let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
        return;
    };

    debug!("disconnecting despawned client `{}`", trigger.target());

    let network_id = clients
        .get(trigger.target())
        .expect("inserted on connection");
    endpoint.try_disconnect_client(network_id.get());
}
