extends SceneTree

const MAIN_SCENE: PackedScene = preload("res://main.tscn")

var _failures: Array[String] = []

func _initialize() -> void:
	call_deferred("_run_contracts")

func _run_contracts() -> void:
	root.size = Vector2i(1440, 900)
	var shell := MAIN_SCENE.instantiate()
	root.add_child(shell)
	await process_frame
	await process_frame

	var left := shell.get_node("ApplicationShell/Workspace/LeftReliquary") as AthanorReliquaryNavigator
	var right := shell.get_node("ApplicationShell/Workspace/RightReliquary") as AthanorReliquaryNavigator
	var scrim := shell.get_node("ApplicationShell/Workspace/DrawerScrim") as Button
	var center := shell.get_node("ApplicationShell/Workspace/CenterFrame") as MarginContainer
	var status := shell.get_node("ApplicationShell/StatusRail") as PanelContainer
	var menu_toggle := shell.get_node("ApplicationShell/StatusRail/Margin/Row/Menu") as Button
	var status_settings := shell.get_node("ApplicationShell/StatusRail/Margin/Row/Settings") as Button
	var context_toggle := shell.get_node("ApplicationShell/StatusRail/Margin/Row/Context") as Button

	_check(left.current_pane() == &"Root", "left reliquary starts at root")
	_check(right.current_pane() == &"Root", "right reliquary starts at context root")
	_check(left.visible and right.visible and not scrim.visible, "wide layout shows four-region rails without scrim")
	_check(center.get_theme_constant("margin_left") == 252, "wide center reserves the left reliquary")
	_check(center.get_theme_constant("margin_right") == 316, "wide center reserves the right reliquary")
	_check(status.custom_minimum_size.y <= 42.0, "bottom status remains a one-row rail")
	var expected_screen_ids := PackedStringArray([
		"S01", "S02", "S07", "S08", "S09", "S14",
	])
	var mapped_screen_ids: Dictionary = {}
	for node: Node in left.find_children("*", "Button", true, false):
		if node.has_meta("screen_id"):
			var screen_id := String(node.get_meta("screen_id"))
			mapped_screen_ids[screen_id] = true
	for screen_id: String in expected_screen_ids:
		_check(mapped_screen_ids.has(screen_id), "operator screen map includes %s" % screen_id)
	_check(mapped_screen_ids.size() == 6, "operator map contains exactly S01, S02, S07, S08, S09, and S14")
	var worker_lanes := left.get_node("Margin/Column/PaneHost/Root/System/Content/WorkerLanes") as Button
	_check(worker_lanes.get_meta("action_id") == &"screen:routing", "operator map includes Worker Lanes by action")
	_check(shell.find_children("OrnamentTop", "*", true, false).is_empty(), "shell does not manufacture unreferenced corner ornament")
	var header_rule := shell.find_child("HeaderRuleTop", true, false)
	var flourish := header_rule.get_node("FlourishStart") as TextureRect
	_check(flourish.texture.resource_path.ends_with("reliquary-divider-flourish.svg"), "screen header uses the exact archived OrnamentFrame flourish")

	var s01_button := left.get_node("Margin/Column/PaneHost/Root/Conversation/Content/S01") as Button
	s01_button.pressed.emit()
	await process_frame
	var route_label := right.get_node("Margin/Column/PaneHost/Root/Content/ActiveRoute/Content/Route") as Label
	_check(route_label.text == "S01 · CONVERSA / RETOMADA", "flat archive navigation routes S01 without adding a nested group layer")

	var right_settings := right.get_node("Margin/Column/PaneHost/Root/Content/Preferences/Content/Settings") as Button
	right_settings.pressed.emit()
	await process_frame
	var appearance_button := right.get_node("Margin/Column/PaneHost/Settings/Appearance") as Button
	appearance_button.grab_focus()
	appearance_button.pressed.emit()
	await process_frame
	_check(right.current_pane() == &"Appearance", "preferences uses the reusable nested stack")
	_check(right.handle_escape(), "first Escape operation consumes one nested level")
	await process_frame
	await process_frame
	_check(right.current_pane() == &"Settings", "Escape returns Appearance to Settings")
	_check(root.gui_get_focus_owner() == appearance_button, "Back restores focus to the nested-pane trigger")
	_check(right.handle_escape(), "second nested Escape returns Settings to Context")
	_check(right.current_pane() == &"Root", "settings stack returns to context root")
	_check(not right.handle_escape(), "root Escape is offered to the shell drawer owner")

	root.size = Vector2i(1000, 760)
	await process_frame
	await process_frame
	_check(left.visible and not right.visible, "compact layout keeps navigation and docks context as a drawer")
	_check(center.get_theme_constant("margin_left") == 232 and center.get_theme_constant("margin_right") == 0, "compact center reserves only the navigation rail")
	context_toggle.pressed.emit()
	await process_frame
	_check(right.visible and scrim.visible, "compact context opens over the center with a scrim")
	(right.get_node("Margin/Column/Header/Close") as Button).pressed.emit()
	await process_frame
	_check(not right.visible and not scrim.visible, "drawer close restores compact reading space")

	root.size = Vector2i(640, 760)
	await process_frame
	await process_frame
	_check(menu_toggle.visible and not left.visible and not right.visible, "narrow layout starts center-only with explicit drawer triggers")
	menu_toggle.pressed.emit()
	await process_frame
	_check(left.visible and scrim.visible, "narrow navigation opens as a reliquary drawer")
	status_settings.pressed.emit()
	await process_frame
	_check(not left.visible and right.visible and right.current_pane() == &"Settings", "settings replaces the navigation drawer with a nested right drawer")
	Input.action_press("ui_cancel")
	await process_frame
	Input.action_release("ui_cancel")
	await process_frame
	_check(right.visible and right.current_pane() == &"Root", "first shell Escape returns nested settings to context root")
	Input.action_press("ui_cancel")
	await process_frame
	Input.action_release("ui_cancel")
	await process_frame
	_check(not right.visible and not scrim.visible, "second shell Escape closes the narrow drawer")

	shell.queue_free()
	await process_frame
	if _failures.is_empty():
		print("RELIQUARY_CONTRACT: PASS · operator map, exact ornament rule, nested panes, focus, escape, layout, rail, drawers")
		quit(0)
	else:
		for failure in _failures:
			push_error("RELIQUARY_CONTRACT: " + failure)
		quit(1)

func _check(condition: bool, description: String) -> void:
	if not condition:
		_failures.append(description)
