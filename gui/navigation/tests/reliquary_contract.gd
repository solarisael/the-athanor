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
	var menu_toggle := shell.get_node("ApplicationShell/TopBar/Margin/Row/MenuToggle") as Button
	var status_settings := shell.get_node("ApplicationShell/StatusRail/Margin/Row/Settings") as Button
	var context_toggle := shell.get_node("ApplicationShell/TopBar/Margin/Row/ContextToggle") as Button

	_check(left.current_pane() == &"Root", "left reliquary starts at root")
	_check(right.current_pane() == &"Root", "right reliquary starts at context root")
	_check(left.visible and right.visible and not scrim.visible, "wide layout shows four-region rails without scrim")
	_check(center.get_theme_constant("margin_left") == 252, "wide center reserves the left reliquary")
	_check(center.get_theme_constant("margin_right") == 316, "wide center reserves the right reliquary")
	_check(status.custom_minimum_size.y <= 42.0, "bottom status remains a one-row rail")

	var conversation_button := left.get_node("Margin/Column/PaneHost/Root/Conversation") as Button
	conversation_button.grab_focus()
	conversation_button.pressed.emit()
	await process_frame
	_check(left.current_pane() == &"Conversation", "pane-target buttons enter nested panes")
	var conversation_pane := left.get_node("Margin/Column/PaneHost/Conversation") as Control
	var memory_pane := left.get_node("Margin/Column/PaneHost/Memory") as Control
	_check(conversation_pane.visible and not memory_pane.visible, "inactive panes are hidden and inert")
	(left.get_node("Margin/Column/Header/Back") as Button).pressed.emit()
	await process_frame
	await process_frame
	_check(left.current_pane() == &"Root", "Back returns one reliquary level")
	_check(root.gui_get_focus_owner() == conversation_button, "Back restores focus to the pane trigger")

	var right_settings := right.get_node("Margin/Column/PaneHost/Root/Settings") as Button
	right_settings.pressed.emit()
	(right.get_node("Margin/Column/PaneHost/Settings/Appearance") as Button).pressed.emit()
	await process_frame
	_check(right.current_pane() == &"Appearance", "settings uses the same reusable nested stack")
	_check(right.handle_escape(), "first Escape operation consumes one nested level")
	_check(right.current_pane() == &"Settings", "Escape returns Appearance to Settings")
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
		print("RELIQUARY_CONTRACT: PASS · nested panes, focus, escape, four-region layout, status rail, drawers")
		quit(0)
	else:
		for failure in _failures:
			push_error("RELIQUARY_CONTRACT: " + failure)
		quit(1)

func _check(condition: bool, description: String) -> void:
	if not condition:
		_failures.append(description)
