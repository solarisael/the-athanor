extends PanelContainer
class_name AthanorReliquaryNavigator

signal action_requested(action_id: StringName)
signal close_requested
signal pane_changed(pane_id: StringName)

@export var pane_host_path: NodePath
@export var title_label_path: NodePath
@export var breadcrumb_label_path: NodePath
@export var back_button_path: NodePath
@export var close_button_path: NodePath
@export var root_pane: StringName = &"Root"

var _pane_host: Control
var _title_label: Label
var _breadcrumb_label: Label
var _back_button: Button
var _close_button: Button
var _stack: Array[StringName] = []
var _focus_returns: Array[Control] = []

func _ready() -> void:
	_pane_host = get_node(pane_host_path) as Control
	_title_label = get_node(title_label_path) as Label
	_breadcrumb_label = get_node(breadcrumb_label_path) as Label
	_back_button = get_node(back_button_path) as Button
	_close_button = get_node(close_button_path) as Button
	_back_button.pressed.connect(back)
	_close_button.pressed.connect(_request_close)
	_wire_buttons(_pane_host)
	open_root()

func open_root() -> void:
	_stack.assign([root_pane])
	_focus_returns.clear()
	_show_pane(root_pane)

func open_pane(pane_id: StringName, return_focus: Control = null) -> bool:
	if _find_pane(pane_id) == null:
		push_error("Reliquary pane does not exist: %s" % pane_id)
		return false
	if _stack.back() == pane_id:
		return true
	_stack.append(pane_id)
	_focus_returns.append(return_focus)
	_show_pane(pane_id)
	return true

func back() -> bool:
	if _stack.size() <= 1:
		return false
	_stack.pop_back()
	var return_focus: Control = _focus_returns.pop_back()
	_show_pane(_stack.back(), false)
	if is_instance_valid(return_focus):
		return_focus.grab_focus.call_deferred()
	return true

func handle_escape() -> bool:
	return back()

func current_pane() -> StringName:
	return _stack.back() if not _stack.is_empty() else root_pane

func set_close_visible(visible: bool) -> void:
	_close_button.visible = visible

func set_active_action(action_id: StringName) -> void:
	_set_active_action_recursive(_pane_host, action_id)

func _show_pane(pane_id: StringName, focus_first := true) -> void:
	var pane := _find_pane(pane_id)
	for child in _pane_host.get_children():
		if child is Control:
			child.visible = child == pane
	_title_label.text = String(pane.get_meta("pane_title", pane.name))
	_breadcrumb_label.text = " / ".join(_stack.map(func(item: StringName) -> String: return String(item).to_upper()))
	_back_button.visible = _stack.size() > 1
	pane_changed.emit(pane_id)
	if focus_first:
		_focus_first.call_deferred(pane)

func _find_pane(pane_id: StringName) -> Control:
	return _pane_host.get_node_or_null(NodePath(String(pane_id))) as Control

func _focus_first(pane: Control) -> void:
	if not is_instance_valid(pane):
		return
	for node in pane.find_children("*", "Button", true, false):
		var button := node as Button
		if button.visible and not button.disabled:
			button.grab_focus()
			return

func _wire_buttons(node: Node) -> void:
	for child in node.get_children():
		if child is Button:
			var button := child as Button
			if button.has_meta("pane_target"):
				button.pressed.connect(_open_from_button.bind(button.get_meta("pane_target"), button))
			elif button.has_meta("action_id"):
				button.pressed.connect(_emit_action.bind(button.get_meta("action_id")))
		_wire_buttons(child)

func _open_from_button(pane_id: StringName, button: Button) -> void:
	open_pane(pane_id, button)

func _emit_action(action_id: StringName) -> void:
	action_requested.emit(action_id)

func _request_close() -> void:
	close_requested.emit()

func _set_active_action_recursive(node: Node, action_id: StringName) -> void:
	for child in node.get_children():
		if child is Button and child.has_meta("action_id"):
			var button := child as Button
			button.theme_type_variation = &"AthanorTabActive" if button.get_meta("action_id") == action_id else &"AthanorTab"
		_set_active_action_recursive(child, action_id)
