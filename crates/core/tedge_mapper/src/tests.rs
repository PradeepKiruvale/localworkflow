use core::time;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use mqtt_channel::{Connection, Message, TopicFilter};
use mqtt_tests::with_timeout::{Maybe, WithTimeout};
use mqtt_tests::StreamExt;
use serial_test::serial;
use tokio::task::JoinHandle;

use crate::mapper::mqtt_config;
use crate::{
    c8y_converter::CumulocityConverter, mapper::create_mapper, size_threshold::SizeThreshold,
};

const ALARM_SYNC_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
#[serial]
async fn c8y_mapper_alarm_mapping_to_smartrest() {
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start the C8Y Mapper
    let c8y_mapper = start_c8y_mapper(broker.port).await.unwrap();

    let _ = broker
        .publish_with_opts(
            "tedge/alarms/major/temperature_alarm",
            r#"{ "message": "Temperature high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    let mut msg = messages
        .next()
        .with_timeout(ALARM_SYNC_TIMEOUT_MS)
        .await
        .expect_or("No message received before timeout");
    dbg!(&msg);

    // The first message could be SmartREST 114 for supported operations
    if msg.contains("114") {
        // Fetch the next message which should be the alarm
        msg = messages
            .next()
            .with_timeout(ALARM_SYNC_TIMEOUT_MS)
            .await
            .expect_or("No message received before timeout");
    }

    // Expect converted temperature alarm message
    dbg!(&msg);
    assert!(msg.contains("302,temperature_alarm"));

    //Clear the previously published alarm
    let _ = broker
        .publish_with_opts(
            "tedge/alarms/major/temperature_alarm",
            "",
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    c8y_mapper.abort();
}

#[tokio::test]
#[serial]
async fn c8y_mapper_syncs_pending_alarms_on_startup() {
    let broker = mqtt_tests::test_mqtt_broker();
    println!("{} starting broker", Utc::now());

    let mut messages = broker.messages_published_on("c8y/s/us").await;
    dbg!("starting mapper");
    let mut internal_messages = broker
        .messages_published_on("c8y-internal/alarms/+/+")
        .await;

    let sub1 = tokio::spawn(async move {
        let topics = vec!["tedge/alarms/+/+", "c8y-internal/alarms/+/+"];
        let mqtt_config = mqtt_channel::Config::default()
            .with_port(55555)
            .with_session_name("test-receiver-1")
            .with_subscriptions(topics.try_into().unwrap());
        let mut con = Connection::new(&mqtt_config).await.unwrap();
        loop {
            match con.received.next().await {
                Some(mesg) => {
                    println!("sub-1: {:?}", mesg);
                }
                _ => {
                    break;
                }
            }
        }
    });

    // Start the C8Y Mapper
    let c8y_mapper = start_c8y_mapper(broker.port).await.unwrap();

    dbg!("publishing  critical/temperature_alarm");
    let _ = broker
        .publish_with_opts(
            "tedge/alarms/critical/temperature_alarm",
            r#"{ "message": "Temperature very high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();
    dbg!("waiting for critical/temperature_alarm");
    let mut msg = messages
        .next()
        .with_timeout(ALARM_SYNC_TIMEOUT_MS)
        .await
        .expect_or("No message received before timeout");

    dbg!(&msg);

    // The first message could be SmartREST 114 for supported operations
    if msg.contains("114") {
        // Fetch the next message which should be the alarm
        msg = messages
            .next()
            .with_timeout(ALARM_SYNC_TIMEOUT_MS)
            .await
            .expect_or("No message received before timeout");
        dbg!(&msg);
    }

    dbg!("asserting critical/temperature_alarm mapping");
    // Expect converted temperature alarm message
    assert!(&msg.contains("301,temperature_alarm"));

    let imsg = internal_messages
        .next()
        .with_timeout(ALARM_SYNC_TIMEOUT_MS)
        .await
        .expect_or("No message received before timeout");
    println!("{}", imsg);
    assert!(&imsg.contains("Temperature very high"));
    tokio::time::sleep(time::Duration::from_secs(10)).await;
    dbg!("stopping mapper");
    c8y_mapper.abort();
    sub1.abort();
    dbg!("publishing critical/pressure_alarm");
    //Publish a new alarm while the mapper is down
    let _ = broker
        .publish_with_opts(
            "tedge/alarms/critical/pressure_alarm",
            r#"{ "message": "Pressure very high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    // Ignored until the rumqttd broker bug that doesn't handle empty retained messages
    //Clear the existing alarm while the mapper is down
    // let _ = broker
    //     .publish_with_opts(
    //         "tedge/alarms/critical/temperature_alarm",
    //         "",
    //         mqtt_channel::QoS::AtLeastOnce,
    //         true,
    //     )
    //     .await
    //     .unwrap();
    tokio::time::sleep(time::Duration::from_secs(20)).await;
    dbg!("restart mapper");
    // Restart the C8Y Mapper
    let _sub1 = tokio::spawn(async move {
        let topics = vec!["tedge/alarms/+/+", "c8y-internal/alarms/+/+"];
        let mqtt_config = mqtt_channel::Config::default()
            .with_port(55555)
            .with_session_name("test-receiver-1")
            .with_subscriptions(topics.try_into().unwrap());

        let mut con = Connection::new(&mqtt_config).await.unwrap();
        loop {
            match con.received.next().await {
                Some(mesg) => {
                    println!("sub-1: {:?}", mesg);
                }
                _ => {
                    break;
                }
            }
        }
    });

    let _sub2 = tokio::spawn(async move {
        let topics = vec!["tedge/alarms/+/+", "c8y-internal/alarms/+/+"];
        let mqtt_config = mqtt_channel::Config::default()
            .with_port(55555)
            .with_session_name("test-receiver-2")
            .with_subscriptions(topics.try_into().unwrap());
        let mut con = Connection::new(&mqtt_config).await.unwrap();
        loop {
            match con.received.next().await {
                Some(mesg) => {
                    println!("sub-2: {:?}", mesg);
                }
                _ => {
                    break;
                }
            }
        }
    });

    let _ = start_c8y_mapper(broker.port).await.unwrap();
    dbg!("waiting for critical/pressure_alarm");

    let mut msg = messages
        .next()
        .with_timeout(ALARM_SYNC_TIMEOUT_MS)
        .await
        .expect_or("No message received before timeout");
    dbg!(&msg);

    // The first message could be SmartREST 114 for supported operations
    if msg.contains("114") {
        // Fetch the next message which should be the alarm
        msg = messages
            .next()
            .with_timeout(ALARM_SYNC_TIMEOUT_MS)
            .await
            .expect_or("No message received before timeout");
        dbg!(&msg);
    }

    // Ignored until the rumqttd broker bug that doesn't handle empty retained messages
    // Expect the previously missed clear temperature alarm message
    // let msg = messages
    //     .next()
    //     .with_timeout(ALARM_SYNC_TIMEOUT_MS)
    //     .await
    //     .expect_or("No message received after a second.");
    // dbg!(&msg);
    // assert!(&msg.contains("306,temperature_alarm"));

    // Expect the new pressure alarm message
    dbg!("asserting critical/pressure_alarm mapping");
    assert!(&msg.contains("301,pressure_alarm"));

    c8y_mapper.abort();
}

async fn start_c8y_mapper(mqtt_port: u16) -> Result<JoinHandle<()>, anyhow::Error> {
    let device_name = "test-device".into();
    let device_type = "test-device-type".into();
    let size_threshold = SizeThreshold(16 * 1024);
    let converter = Box::new(CumulocityConverter::new(
        size_threshold,
        device_name,
        device_type,
    ));

    let mut mapper = create_mapper("c8y-mapper-test", mqtt_port, converter).await?;

    let mapper_task = tokio::spawn(async move {
        let _ = mapper.run().await;
    });
    Ok(mapper_task)
}
