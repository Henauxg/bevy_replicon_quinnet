use std::{
    net::{IpAddr, Ipv6Addr},
    thread::sleep,
    time::Duration,
};

use bevy::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, state::app::StatesPlugin};
use bevy_quinnet::{
    client::{
        certificate::CertificateVerificationMode, connection::ClientAddrConfiguration,
        ClientConnectionConfiguration, ClientConnectionConfigurationDefaultables, QuinnetClient,
    },
    server::{
        certificate::CertificateRetrievalMode, EndpointAddrConfiguration, QuinnetServer,
        ServerEndpointConfiguration, ServerEndpointConfigurationDefaultables,
    },
};
use bevy_replicon::prelude::*;
use bevy_replicon_quinnet::{ChannelsConfigurationExt, RepliconQuinnetPlugins};
use serde::{Deserialize, Serialize};

#[test]
fn connect_disconnect() {
    let port = 6000; // TODO Use port 0 and retrieve the port used by the server.
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin {
                tick_schedule: PostUpdate.intern(),
                ..Default::default()
            }),
            RepliconQuinnetPlugins,
        ))
        .finish();
    }

    setup(&mut server_app, &mut client_app, port);

    let server_state = server_app.world().resource::<State<ServerState>>();
    assert_eq!(*server_state, ServerState::Running);

    let quinnet_server = server_app.world().resource::<QuinnetServer>();
    assert_eq!(quinnet_server.endpoint().clients().len(), 1);

    // // TODO Better way to wait a bit more for `AuthorizedClient`component insertion. Maybe wait on `ProtocolHash` event ?
    sleep(Duration::from_secs_f32(0.05));
    server_app.update();

    let mut clients = server_app
        .world_mut()
        .query::<(&ConnectedClient, &AuthorizedClient)>();
    assert_eq!(clients.iter(server_app.world()).len(), 1);

    let client_state = client_app.world().resource::<State<ClientState>>();
    assert_eq!(*client_state, ClientState::Connected);

    let mut quinnet_client = client_app.world_mut().resource_mut::<QuinnetClient>();
    assert!(quinnet_client.is_connected());

    let default_connection = quinnet_client.get_default_connection().unwrap();
    quinnet_client.close_connection(default_connection).unwrap();

    client_app.update();

    server_wait_for_disconnect(&mut server_app);

    assert_eq!(clients.iter(server_app.world()).len(), 0);

    let client_state = client_app.world().resource::<State<ClientState>>();
    assert_eq!(*client_state, ClientState::Disconnected);

    let mut quinnet_server = server_app.world_mut().resource_mut::<QuinnetServer>();
    assert_eq!(quinnet_server.endpoint().clients().len(), 0);

    quinnet_server.stop_endpoint().unwrap();

    server_app.update();

    let server_state = server_app.world().resource::<State<ServerState>>();
    assert_eq!(
        *server_state,
        ServerState::Stopped,
        "requires resource removal"
    );
}

#[test]
fn disconnect_request() {
    let port = 6001; // TODO Use port 0 and retrieve the port used by the server.
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin {
                tick_schedule: PostUpdate.intern(),
                ..Default::default()
            }),
            RepliconQuinnetPlugins,
        ))
        .add_server_message::<Test>(Channel::Ordered)
        .finish();
    }

    setup(&mut server_app, &mut client_app, port);

    // TODO (Pending messages delivery on disconnect) Currently, disconnecting does not deliver pending messages reliably enough to be tested.
    // If we wanted to test this, we'd need not to drop the InternalConnectionRef immediately in Quinnet when disconnecting a client from the server.

    server_app.world_mut().spawn(Replicated);
    server_app.world_mut().write_message(ToClients {
        mode: SendMode::Broadcast,
        message: Test,
    });

    let mut clients = server_app
        .world_mut()
        .query_filtered::<Entity, With<ConnectedClient>>();
    let client = clients.single(server_app.world()).unwrap();
    server_app
        .world_mut()
        .write_message(DisconnectRequest { client });

    server_app.update();

    assert_eq!(clients.iter(server_app.world()).len(), 0);

    // TODO Better way to wait for disconnect propagation
    sleep(Duration::from_secs_f32(0.05));

    server_app.update();
    client_app.update();

    let client_state = client_app.world().resource::<State<ClientState>>();
    assert_eq!(*client_state, ClientState::Disconnected);

    // TODO (Pending messages delivery on disconnect)
    // let events = client_app.world().resource::<Events<TestEvent>>();
    // assert_eq!(events.len(), 1, "last event should be received");

    // let mut replicated = client_app.world_mut().query::<&Replicated>();
    // assert_eq!(
    //     replicated.iter(client_app.world()).len(),
    //     1,
    //     "last replication should be received"
    // );
}

#[test]
fn replication() {
    let port = 6002; // TODO Use port 0 and retrieve the port used by the server.
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin {
                tick_schedule: PostUpdate.intern(),
                ..Default::default()
            }),
            RepliconQuinnetPlugins,
        ))
        .finish();
    }

    setup(&mut server_app, &mut client_app, port);

    server_app.world_mut().spawn(Replicated);

    server_app.update();
    client_wait_for_message(&mut client_app);

    let mut replicated = client_app.world_mut().query::<&Replicated>();
    assert_eq!(replicated.iter(client_app.world()).len(), 1);
}

#[test]
fn server_event() {
    let port = 6003; // TODO Use port 0 and retrieve the port used by the server.
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin {
                tick_schedule: PostUpdate.intern(),
                ..Default::default()
            }),
            RepliconQuinnetPlugins,
        ))
        .add_server_message::<Test>(Channel::Ordered)
        .finish();
    }

    setup(&mut server_app, &mut client_app, port);

    server_app.world_mut().write_message(ToClients {
        mode: SendMode::Broadcast,
        message: Test,
    });

    server_app.update();
    client_wait_for_message(&mut client_app);

    let messages = client_app.world().resource::<Messages<Test>>();
    assert_eq!(messages.len(), 1);
}

#[test]
fn client_event() {
    let port = 6004; // TODO Use port 0 and retrieve the port used by the server.
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin {
                tick_schedule: PostUpdate.intern(),
                ..Default::default()
            }),
            RepliconQuinnetPlugins,
        ))
        .add_client_message::<Test>(Channel::Ordered)
        .finish();
    }

    let client_id = setup(&mut server_app, &mut client_app, port);

    let server_state = server_app.world().resource::<State<ServerState>>();
    assert_eq!(*server_state, ServerState::Running);

    client_app.world_mut().write_message(Test);

    client_app.update();
    server_wait_for_message(&mut server_app, client_id);

    let client_messages = server_app.world().resource::<Messages<FromClient<Test>>>();
    assert_eq!(client_messages.len(), 1);
}

fn setup(
    server_app: &mut App,
    client_app: &mut App,
    server_port: u16,
) -> bevy_quinnet::shared::ClientId {
    setup_server(server_app, server_port);
    setup_client(client_app, server_port);
    wait_for_connection(server_app, client_app)
}

fn setup_client(app: &mut App, server_port: u16) {
    let send_channels_cfg = app.world().resource::<RepliconChannels>().client_configs();

    let mut client = app.world_mut().resource_mut::<QuinnetClient>();
    client
        .open_connection(ClientConnectionConfiguration {
            addr_config: ClientAddrConfiguration::from_ips(
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                server_port,
                IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)),
                0,
            ),
            cert_mode: CertificateVerificationMode::SkipVerification,
            defaultables: ClientConnectionConfigurationDefaultables { send_channels_cfg },
        })
        .unwrap();
}

fn setup_server(app: &mut App, server_port: u16) {
    let send_channels_cfg = app.world().resource::<RepliconChannels>().server_configs();

    let mut server = app.world_mut().resource_mut::<QuinnetServer>();
    server
        .start_endpoint(ServerEndpointConfiguration {
            addr_config: EndpointAddrConfiguration::from_ip(
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                server_port,
            ),
            cert_mode: CertificateRetrievalMode::GenerateSelfSigned {
                server_hostname: Ipv6Addr::LOCALHOST.to_string(),
            },
            defaultables: ServerEndpointConfigurationDefaultables { send_channels_cfg },
        })
        .unwrap();
}

fn wait_for_connection(
    server_app: &mut App,
    client_app: &mut App,
) -> bevy_quinnet::shared::ClientId {
    loop {
        client_app.update();
        server_app.update();
        if client_app
            .world()
            .resource::<QuinnetClient>()
            .is_connected()
        {
            client_app.update();
            server_app.update();
            break;
        }
    }
    client_app
        .world()
        .resource::<QuinnetClient>()
        .connection()
        .client_id()
        .unwrap()
}

fn client_wait_for_message(client_app: &mut App) {
    loop {
        sleep(Duration::from_secs_f32(0.05));
        client_app.update();
        if client_app
            .world()
            .resource::<QuinnetClient>()
            .connection()
            .stats()
            .received_messages_count()
            > 0
        {
            break;
        }
    }
}

fn server_wait_for_message(server_app: &mut App, client_id: bevy_quinnet::shared::ClientId) {
    loop {
        sleep(Duration::from_secs_f32(0.05));
        server_app.update();
        if server_app
            .world()
            .resource::<QuinnetServer>()
            .endpoint()
            .connection(client_id)
            .unwrap()
            .stats()
            .received_messages_count()
            > 0
        {
            break;
        }
    }
}

fn server_wait_for_disconnect(server_app: &mut App) {
    loop {
        sleep(Duration::from_secs_f32(0.05));
        server_app.update();
        if server_app
            .world()
            .resource::<QuinnetServer>()
            .endpoint()
            .endpoint_stats()
            .disconnect_count()
            > 0
        {
            break;
        }
    }
}

#[derive(Deserialize, Message, Serialize)]
struct Test;
