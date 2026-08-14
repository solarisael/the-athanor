extends SceneTree

const LAB_SCENE: PackedScene = preload("res://effects_lab/effects_lab.tscn")
const PHASE_STOKE: Resource = preload("res://effects_lab/phase_stoke.tres")
const CHROMATIC_LAYER := &"_juicee_chromatic_overlay"

var _failures: Array[String] = []

func _initialize() -> void:
	call_deferred("_run_contracts")

func _run_contracts() -> void:
	root.size = Vector2i(1100, 760)
	var lab := LAB_SCENE.instantiate()
	root.add_child(lab)
	await process_frame
	await process_frame

	var cards: Array[Control] = lab.contract_cards()
	_check(cards.size() == 4, "four source-labelled phase treatments exist")
	_check(int(ProjectSettings.get_setting("display/window/vsync/vsync_mode")) == DisplayServer.VSYNC_DISABLED, "actual project disables VSync")
	var card := cards[0]
	var original_modulate := card.modulate
	var original_scale := card.scale

	lab.contract_set_accessibility(false, true, false)
	lab.play_overlap_contract()
	await create_timer(0.18).timeout
	var overlay_count: int = lab.contract_overlay_count(CHROMATIC_LAYER)
	_check(overlay_count == 1, "overlapping chromatic effects keep exactly one overlay")
	_check(lab.contract_active_runs() == 3, "overlap uses three independent sequence instances")

	lab.cancel_all_runs()
	await process_frame
	await process_frame
	_check(lab.contract_overlay_count(CHROMATIC_LAYER) == 0, "cancellation removes every chromatic overlay")
	_check(card.modulate.is_equal_approx(original_modulate), "cancellation restores modulate")
	_check(card.scale.is_equal_approx(original_scale), "cancellation restores scale")

	lab.contract_set_accessibility(true, true, true)
	var gated_sequence: Resource = PHASE_STOKE.duplicate(true)
	var flash_effect: Resource
	var chromatic_effect: Resource
	for effect in gated_sequence.effects:
		var path: String = effect.get_script().resource_path
		if path.ends_with("/flash_effect.gd"):
			flash_effect = effect
		elif path.ends_with("/chromatic_effect.gd"):
			chromatic_effect = effect
	_check(flash_effect != null and chromatic_effect != null, "real sequence contains flash and chromatic accessibility gates")
	if flash_effect and chromatic_effect:
		flash_effect.apply(card)
		chromatic_effect.apply(card)
		await process_frame
		_check(card.modulate.is_equal_approx(original_modulate), "no-flash blocks the flash effect before mutation")
		_check(lab.contract_overlay_count(CHROMATIC_LAYER) == 0, "no-chromatic blocks overlay creation")

	lab.play_overlap_contract()
	await create_timer(0.22).timeout
	_check(card.scale.is_equal_approx(original_scale), "reduced-motion prunes scale motion from duplicated runs")
	_check(lab.contract_overlay_count(CHROMATIC_LAYER) == 0, "accessibility gates survive overlapping runs")
	lab.cancel_all_runs()
	await process_frame

	root.size = Vector2i(640, 760)
	await process_frame
	await process_frame
	lab.contract_apply_layout()
	var narrow_width: float = cards[0].custom_minimum_size.x
	_check(narrow_width <= 544.0 and narrow_width >= 500.0, "narrow layout fits one card inside viewport margins (observed %.1f)" % narrow_width)

	root.size = Vector2i(1440, 900)
	await process_frame
	await process_frame
	lab.contract_apply_layout()
	var wide_width: float = cards[0].custom_minimum_size.x
	_check(wide_width >= 320.0 and wide_width <= 330.0, "wide layout resolves to four measured columns (observed %.1f)" % wide_width)

	lab.contract_set_accessibility(false, true, false)
	lab.play_selected_phase()
	await create_timer(1.0).timeout
	await process_frame
	_check(lab.contract_active_runs() == 0, "completed sequences retire themselves")
	_check(lab.contract_overlay_count(CHROMATIC_LAYER) == 0, "completed sequences leave no stale overlay")
	_check(card.modulate.is_equal_approx(original_modulate), "completed sequence restores modulate")
	_check(card.scale.is_equal_approx(original_scale), "completed sequence restores scale")

	lab.queue_free()
	await process_frame
	if _failures.is_empty():
		print("EFFECTS_LAB_CONTRACT: PASS · overlap, cleanup, accessibility, restoration, resize")
		quit(0)
	else:
		for failure in _failures:
			push_error("EFFECTS_LAB_CONTRACT: " + failure)
		quit(1)

func _check(condition: bool, description: String) -> void:
	if not condition:
		_failures.append(description)
