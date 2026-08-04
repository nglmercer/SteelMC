use super::*;

fn vec2_dispatcher() -> TestDispatcher {
    let mut dispatcher = TestDispatcher::new();
    let command = literal("coordinates")
        .then(argument("value", SteelArgumentType::vec2(true)).executes(|_| Ok(1)));
    assert!(dispatcher.register(command).is_ok());
    dispatcher
}

fn parsed(input: &str) -> Result<Coordinates, CommandSyntaxError> {
    let dispatcher = vec2_dispatcher();
    let parse = dispatcher.parse(input, TestSource::new());
    let chain = dispatcher.context_chain(parse)?;
    chain
        .top_context()
        .coordinates("value")
        .ok_or_else(|| CommandSyntaxError::dynamic("coordinates were not retained"))
}

/// Vanilla builds the pair as a 3D coordinate whose `y` is a relative zero, so the height
/// resolves against the source rather than the argument.
#[test]
fn vec2_fills_the_height_with_a_relative_zero() {
    assert_eq!(
        parsed("coordinates 3 7"),
        Ok(Coordinates::World(WorldCoordinates::new(
            WorldCoordinate::new(false, 3.5),
            WorldCoordinate::new(true, 0.0),
            WorldCoordinate::new(false, 7.5),
        )))
    );
}

#[test]
fn vec2_keeps_relative_components_relative() {
    assert_eq!(
        parsed("coordinates ~2 ~-3"),
        Ok(Coordinates::World(WorldCoordinates::new(
            WorldCoordinate::new(true, 2.0),
            WorldCoordinate::new(true, 0.0),
            WorldCoordinate::new(true, -3.0),
        )))
    );
}

#[test]
fn vec2_rejects_an_incomplete_or_local_pair() {
    assert!(parsed("coordinates 3").is_err());
    // Local coordinates need a facing, which a two-component argument cannot express.
    assert!(parsed("coordinates ^1 ^2").is_err());
}
