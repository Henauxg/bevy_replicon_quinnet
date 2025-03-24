/*!
Provides integration for [`bevy_replicon`](https://docs.rs/bevy_replicon) for [`bevy_quinnet`](https://docs.rs/bevy_quinnet).
*/

use bevy::{app::PluginGroupBuilder, prelude::*};
use bevy_quinnet::shared::channels::{
    ChannelKind, ChannelsConfiguration, DEFAULT_MAX_RELIABLE_FRAME_LEN,
};
use bevy_replicon::prelude::*;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
use client::RepliconQuinnetClientPlugin;
#[cfg(feature = "server")]
use server::RepliconQuinnetServerPlugin;

pub const BYTES_PER_SEC_PERIOD: f64 = 0.1;

pub struct RepliconQuinnetPlugins;

impl PluginGroup for RepliconQuinnetPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        #[cfg(feature = "server")]
        {
            group = group.add(RepliconQuinnetServerPlugin);
        }

        #[cfg(feature = "client")]
        {
            group = group.add(RepliconQuinnetClientPlugin);
        }

        group
    }
}

pub trait ChannelsConfigurationExt {
    /// Returns server channel configs that can be used to start an endpoint on the [`bevy_quinnet::server::QuinnetServer`].
    fn server_configs(&self) -> ChannelsConfiguration;

    /// Same as [ChannelsConfigurationExt::server_configs] with custom configuration of `max_reliable_payload_size` used to configure Quinnet's [ChannelKind]
    fn server_configs_custom(&self, max_reliable_payload_size: usize) -> ChannelsConfiguration;

    /// Same as [`ChannelsConfigurationExt::server_configs`], but for clients.
    fn client_configs(&self) -> ChannelsConfiguration;

    /// Same as [ChannelsConfigurationExt::client_configs] with custom configuration of `max_reliable_payload_size` used to configure Quinnet's [ChannelKind]
    fn client_configs_custom(&self, max_reliable_payload_size: usize) -> ChannelsConfiguration;
}
impl ChannelsConfigurationExt for RepliconChannels {
    fn server_configs(&self) -> ChannelsConfiguration {
        self.server_configs_custom(DEFAULT_MAX_RELIABLE_FRAME_LEN)
    }

    fn server_configs_custom(&self, max_reliable_payload_size: usize) -> ChannelsConfiguration {
        let channels = self.server_channels();
        if channels.len() > u8::MAX as usize {
            panic!("number of server channels shouldn't exceed `u8::MAX`");
        }
        create_configs(channels, max_reliable_payload_size)
    }

    fn client_configs(&self) -> ChannelsConfiguration {
        self.client_configs_custom(DEFAULT_MAX_RELIABLE_FRAME_LEN)
    }

    fn client_configs_custom(&self, max_reliable_payload_size: usize) -> ChannelsConfiguration {
        let channels = self.client_channels();
        if channels.len() > u8::MAX as usize {
            panic!("number of server channels shouldn't exceed `u8::MAX`");
        }
        create_configs(channels, max_reliable_payload_size)
    }
}

/// Converts replicon channels into quinnet channel configs.
fn create_configs(channels: &[Channel], max_frame_size: usize) -> ChannelsConfiguration {
    let mut quinnet_channels = ChannelsConfiguration::new();
    for channel in channels.iter() {
        quinnet_channels.add(match channel {
            Channel::Unreliable => ChannelKind::Unreliable,
            Channel::Unordered => ChannelKind::UnorderedReliable { max_frame_size },
            Channel::Ordered => ChannelKind::OrderedReliable { max_frame_size },
        });
    }
    quinnet_channels
}
