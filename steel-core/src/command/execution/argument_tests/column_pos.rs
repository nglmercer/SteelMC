use super::*;

fn parsed(input: &str) -> Result<Coordinates, CommandSyntaxError> {
    let mut dispatcher = TestDispatcher::new();
    let command = literal("coordinates")
        .then(argument("value", SteelArgumentType::column_pos()).executes(|_| Ok(1)));
    assert!(dispatcher.register(command).is_ok());
    let parse = dispatcher.parse(input, TestSource::new());
    let chain = dispatcher.context_chain(parse)?;
    chain
        .top_context()
        .coordinates("value")
        .ok_or_else(|| CommandSyntaxError::dynamic("coordinates were not retained"))
}

/// Like vec2, the pair becomes a 3D coordinate with a relative-zero height.
#[test]
fn column_pos_fills_the_height_with_a_relative_zero() {
    assert_eq!(
        parsed("coordinates 3 7"),
        Ok(Coordinates::World(WorldCoordinates::new(
            WorldCoordinate::new(false, 3.0),
            WorldCoordinate::new(true, 0.0),
            WorldCoordinate::new(false, 7.0),
        )))
    );
}

/// Unlike vec2 the components are whole blocks, so they are not centre-corrected.
#[test]
fn column_pos_does_not_centre_correct() {
    let Ok(Coordinates::World(coordinates)) = parsed("coordinates 3 7") else {
        panic!("a column position should parse");
    };
    assert_eq!(
        coordinates,
        WorldCoordinates::new(
            WorldCoordinate::new(false, 3.0),
            WorldCoordinate::new(true, 0.0),
            WorldCoordinate::new(false, 7.0),
        )
    );
}

#[test]
fn column_pos_keeps_relative_components_relative() {
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
fn column_pos_rejects_an_incomplete_pair() {
    assert!(parsed("coordinates 3").is_err());
}
