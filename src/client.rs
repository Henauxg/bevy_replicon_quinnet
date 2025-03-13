use bevy::{
    app::{App, Plugin, PostUpdate, PreUpdate},
    prelude::{IntoSystemConfigs, IntoSystemSetConfigs, Local, Res, ResMut},
    time::Time,
};
use bevy_quinnet::{
    client::{QuinnetClient, QuinnetClientPlugin},
    shared::QuinnetSyncUpdate,
};
use bevy_replicon::{
    client::ClientSet,
    prelude::{RepliconClient, RepliconClientStatus},
};

use crate::BYTES_PER_SEC_PERIOD;

pub struct RepliconQuinnetClientPlugin;

impl Plugin for RepliconQuinnetClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(QuinnetClientPlugin::default())
            .configure_sets(
                PreUpdate,
                ClientSet::ReceivePackets.after(QuinnetSyncUpdate),
            )
            .add_systems(
                PreUpdate,
                (
                    Self::set_connecting.run_if(bevy_quinnet::client::client_connecting),
                    Self::set_disconnected.run_if(bevy_quinnet::client::client_just_disconnected),
                    Self::set_connected.run_if(bevy_quinnet::client::client_just_connected),
                    (Self::receive_packets, Self::update_statistics)
                        .run_if(bevy_quinnet::client::client_connected),
                )
                    .chain()
                    .in_set(ClientSet::ReceivePackets),
            )
            .add_systems(
                PostUpdate,
                Self::send_packets
                    .in_set(ClientSet::SendPackets)
                    .run_if(bevy_quinnet::client::client_connected),
            );
    }
}

impl RepliconQuinnetClientPlugin {
    fn set_disconnected(mut client: ResMut<RepliconClient>) {
        client.set_status(RepliconClientStatus::Disconnected);
    }

    fn set_connecting(mut client: ResMut<RepliconClient>) {
        client.set_status(RepliconClientStatus::Connecting);
    }

    fn set_connected(mut client: ResMut<RepliconClient>) {
        client.set_status(RepliconClientStatus::Connected);
    }

    fn update_statistics(
        mut bps_timer: Local<f64>,
        mut quinnet_client: ResMut<QuinnetClient>,
        mut replicon_client: ResMut<RepliconClient>,
        time: Res<Time>,
    ) {
        let Some(con) = quinnet_client.get_connection_mut() else {
            return;
        };
        let Some(stats) = con.connection_stats() else {
            return;
        };

        let client_stats = replicon_client.stats_mut();
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

    fn receive_packets(
        mut quinnet_client: ResMut<QuinnetClient>,
        mut replicon_client: ResMut<RepliconClient>,
    ) {
        let Some(connection) = quinnet_client.get_connection_mut() else {
            return;
        };

        while let Some((channel_id, message)) = connection.try_receive_payload() {
            replicon_client.insert_received(channel_id, message);
        }
    }

    fn send_packets(
        mut quinnet_client: ResMut<QuinnetClient>,
        mut replicon_client: ResMut<RepliconClient>,
    ) {
        let Some(connection) = quinnet_client.get_connection_mut() else {
            return;
        };
        for (channel_id, message) in replicon_client.drain_sent() {
            connection.try_send_payload_on(channel_id, message);
        }
    }
}
