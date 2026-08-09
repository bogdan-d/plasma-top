use super::*;

#[test]
fn form_defaults_to_value() {
    assert_eq!(Form::default(), Form::Value);
    assert_eq!("".parse::<Form>(), Ok(Form::Value));
}

#[test]
fn panel_forms_stay_out_of_tooltip() {
    assert!(!Form::Bar.allowed_surfaces().contains(Surface::Tooltip));
    assert!(
        Form::Bar
            .allowed_surfaces()
            .contains(Surface::PanelHorizontal)
    );
    assert!(
        Form::Spark
            .allowed_surfaces()
            .contains(Surface::PanelVertical)
    );
}

#[test]
fn tooltip_only_forms_stay_out_of_panel() {
    let surfaces = Form::SparkValue.allowed_surfaces();

    assert!(surfaces.contains(Surface::Tooltip));
    assert!(!surfaces.contains(Surface::PanelHorizontal));
    assert!(!surfaces.contains(Surface::PanelVertical));
}
