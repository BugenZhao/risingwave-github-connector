use futures::{pin_mut, TryStreamExt};

#[tokio::test]
async fn events() {
    let crab = octocrab::OctocrabBuilder::new()
        .base_url("https://api.github.com/repos/risingwavelabs/risingwave/")
        .unwrap()
        .build()
        .unwrap();

    let events = crab.events().send().await.unwrap();
    let value = events.value.unwrap();
    let stream = value.into_stream(&crab);
    pin_mut!(stream);

    while let Some(item) = stream.try_next().await.unwrap() {
        println!("{:?}", item.actor.login);
    }
}
