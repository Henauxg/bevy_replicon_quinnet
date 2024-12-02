use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    prelude::{
        EventReader, EventWriter, IntoSystemConfigs, IntoSystemSetConfigs, Local, Res, ResMut,
    },
    time::Time,
};
use bevy_quinnet::{
    server::{QuinnetServer, QuinnetServerPlugin},
    shared::QuinnetSyncUpdate,
};
use bevy_replicon::{
    core::ClientId,
    prelude::{ConnectedClients, RepliconServer},
    server::{ServerEvent, ServerSet},
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
            .add_systems(
                PreUpdate,
                (
                    (
                        Self::set_running.run_if(bevy_quinnet::server::server_just_opened),
                        Self::set_stopped.run_if(bevy_quinnet::server::server_just_closed),
                        (Self::receive_packets, Self::update_statistics)
                            .run_if(bevy_quinnet::server::server_listening),
                    )
                        .chain()
                        .in_set(ServerSet::ReceivePackets),
                    Self::forward_server_events.in_set(ServerSet::SendEvents),
                ),
            )
            .add_systems(
                PostUpdate,
                Self::send_packets
                    .in_set(ServerSet::SendPackets)
                    .run_if(bevy_quinnet::server::server_listening),
            );
    }
}

impl RepliconQuinnetServerPlugin {
    fn set_running(mut server: ResMut<RepliconServer>) {
        server.set_running(true);
    }

    fn set_stopped(mut server: ResMut<RepliconServer>) {
        server.set_running(false);
    }

    fn update_statistics(
        mut bps_timer: Local<f64>,
        mut connected_clients: ResMut<ConnectedClients>,
        mut quinnet_server: ResMut<QuinnetServer>,
        time: Res<Time>,
    ) {
        let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
            return;
        };
        for client in connected_clients.iter_mut() {
            let Some(con) = endpoint.get_connection_mut(client.id().get()) else {
                return;
            };
            let stats = con.connection_stats();

            client.set_rtt(stats.path.rtt.as_secs_f64());
            client.set_packet_loss(
                100. * (stats.path.lost_packets as f64 / stats.path.sent_packets as f64),
            );

            *bps_timer += time.delta_secs_f64();
            if *bps_timer >= BYTES_PER_SEC_PERIOD {
                *bps_timer = 0.;
                let received_bytes_count = con.clear_received_bytes_count() as f64;
                let sent_bytes_count = con.clear_sent_bytes_count() as f64;
                client.set_received_bps(received_bytes_count / BYTES_PER_SEC_PERIOD);
                client.set_sent_bps(sent_bytes_count / BYTES_PER_SEC_PERIOD);
            }
        }
    }

    fn forward_server_events(
        mut conn_events: EventReader<bevy_quinnet::server::ConnectionEvent>,
        mut conn_lost_events: EventReader<bevy_quinnet::server::ConnectionLostEvent>,
        mut server_events: EventWriter<ServerEvent>,
    ) {
        for event in conn_events.read() {
            server_events.send(ServerEvent::ClientConnected {
                client_id: ClientId::new(event.id),
            });
        }
        for event in conn_lost_events.read() {
            server_events.send(ServerEvent::ClientDisconnected {
                client_id: ClientId::new(event.id),
                reason: "".to_string(),
            });
        }
    }

    fn receive_packets(
        connected_clients: Res<ConnectedClients>,
        mut quinnet_server: ResMut<QuinnetServer>,
        mut replicon_server: ResMut<RepliconServer>,
    ) {
        let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
            return;
        };
        for &client in connected_clients.iter() {
            while let Some((channel_id, message)) =
                endpoint.try_receive_payload_from(client.id().get())
            {
                replicon_server.insert_received(client.id(), channel_id, message);
            }
        }
    }

    fn send_packets(
        mut quinnet_server: ResMut<QuinnetServer>,
        mut replicon_server: ResMut<RepliconServer>,
    ) {
        let Some(endpoint) = quinnet_server.get_endpoint_mut() else {
            return;
        };
        for (client_id, channel_id, message) in replicon_server.drain_sent() {
            endpoint.try_send_payload_on(client_id.get(), channel_id, message);
        }
    }
}
