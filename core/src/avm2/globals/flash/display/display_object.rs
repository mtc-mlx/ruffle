//! `flash.display.DisplayObject` builtin/prototype

use crate::avm2::activation::Activation;
use crate::avm2::error::{argument_error, illegal_operation_error, make_error_2008, type_error};
use crate::avm2::filters::FilterAvm2Ext;
use crate::avm2::object::{Object, TObject};
use crate::avm2::parameters::ParametersExt;
use crate::avm2::value::Value;
use crate::avm2::StageObject;
use crate::avm2::{ArrayObject, ArrayStorage};
use crate::avm2::{ClassObject, Error};
use crate::display_object::{DisplayObject, HitTestOptions, TDisplayObject};
use crate::ecma_conversions::round_to_even;
use crate::prelude::*;
use crate::string::AvmString;
use crate::types::{Degrees, Percent};
use crate::vminterface::Instantiator;
use crate::{avm2_stub_getter, avm2_stub_setter};
use ruffle_render::blend::ExtendedBlendMode;
use ruffle_render::filters::Filter;
use std::str::FromStr;
use swf::Rectangle;
use swf::Twips;

pub fn display_object_allocator<'gc>(
    class: ClassObject<'gc>,
    activation: &mut Activation<'_, 'gc>,
) -> Result<Object<'gc>, Error<'gc>> {
    let class_name = class.inner_class_definition().read().name().local_name();

    return Err(Error::AvmError(argument_error(
        activation,
        &format!("Error #2012: {class_name}$ class cannot be instantiated."),
        2012,
    )?));
}

/// Initializes a DisplayObject created from ActionScript.
/// This should be called from the AVM2 class's native allocator
/// (e.g. `sprite_allocator`)
pub fn initialize_for_allocator<'gc>(
    activation: &mut Activation<'_, 'gc>,
    dobj: DisplayObject<'gc>,
    class: ClassObject<'gc>,
) -> Result<Object<'gc>, Error<'gc>> {
    let obj: StageObject = StageObject::for_display_object(activation, dobj, class)?;
    dobj.set_placed_by_script(activation.context.gc_context, true);
    dobj.set_object2(&mut activation.context, obj.into());

    // [NA] Should these run for everything?
    dobj.post_instantiation(&mut activation.context, None, Instantiator::Avm2, false);
    dobj.enter_frame(&mut activation.context);
    dobj.construct_frame(&mut activation.context);

    // Movie clips created from ActionScript skip the next enterFrame,
    // and consequently are observed to have their currentFrame lag one
    // frame behind objects placed by the timeline (even if they were
    // both placed in the same frame to begin with).
    dobj.base_mut(activation.context.gc_context)
        .set_skip_next_enter_frame(true);
    dobj.on_construction_complete(&mut activation.context);

    Ok(obj.into())
}

/// Implements `flash.display.DisplayObject`'s native instance constructor.
pub fn native_instance_init<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    activation.super_init(this, &[])?;

    if let Some(dobj) = this.as_display_object() {
        if let Some(clip) = dobj.as_movie_clip() {
            clip.set_constructing_frame(true, activation.context.gc_context);
        }

        if let Some(container) = dobj.as_container() {
            for child in container.iter_render_list() {
                child.construct_frame(&mut activation.context);
            }
        }

        if let Some(clip) = dobj.as_movie_clip() {
            clip.set_constructing_frame(false, activation.context.gc_context);
        }
    }

    Ok(Value::Undefined)
}

/// Implements `alpha`'s getter.
pub fn get_alpha<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.alpha().into());
    }

    Ok(Value::Undefined)
}

/// Implements `alpha`'s setter.
pub fn set_alpha<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_alpha = args.get_f64(activation, 0)?;
        dobj.set_alpha(activation.context.gc_context, new_alpha);
    }

    Ok(Value::Undefined)
}

/// Implements `height`'s getter.
pub fn get_height<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.height().into());
    }

    Ok(Value::Undefined)
}

/// Implements `height`'s setter.
pub fn set_height<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_height = args.get_f64(activation, 0)?;
        if new_height >= 0.0 {
            dobj.set_height(&mut activation.context, new_height);
        }
    }

    Ok(Value::Undefined)
}

/// Implements `scale9Grid`'s getter.
pub fn get_scale9grid<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "scale9Grid");
    if let Some(dobj) = this.as_display_object() {
        let rect = dobj.scaling_grid();
        return if rect.is_valid() {
            let rect = new_rectangle(activation, rect)?;
            Ok(rect.into())
        } else {
            Ok(Value::Null)
        };
    }

    Ok(Value::Undefined)
}

/// Implements `scale9Grid`'s setter.
pub fn set_scale9grid<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "scale9Grid");
    if let Some(dobj) = this.as_display_object() {
        let rect = match args.try_get_object(activation, 0) {
            None => Rectangle::default(),
            Some(rect) => object_to_rectangle(activation, rect)?,
        };
        dobj.set_scaling_grid(activation.context.gc_context, rect);
    }

    Ok(Value::Undefined)
}

/// Implements `scaleY`'s getter.
pub fn get_scale_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.scale_y(activation.context.gc_context).unit().into());
    }

    Ok(Value::Undefined)
}

/// Implements `scaleY`'s setter.
pub fn set_scale_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_scale = args.get_f64(activation, 0)?;
        dobj.set_scale_y(activation.context.gc_context, Percent::from_unit(new_scale));
    }

    Ok(Value::Undefined)
}

/// Implements `width`'s getter.
pub fn get_width<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.width().into());
    }

    Ok(Value::Undefined)
}

/// Implements `width`'s setter.
pub fn set_width<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_width = args.get_f64(activation, 0)?;
        if new_width >= 0.0 {
            dobj.set_width(&mut activation.context, new_width);
        }
    }

    Ok(Value::Undefined)
}

/// Implements `scaleX`'s getter.
pub fn get_scale_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.scale_x(activation.context.gc_context).unit().into());
    }

    Ok(Value::Undefined)
}

/// Implements `scaleX`'s setter.
pub fn set_scale_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_scale = args.get_f64(activation, 0)?;
        dobj.set_scale_x(activation.context.gc_context, Percent::from_unit(new_scale));
    }

    Ok(Value::Undefined)
}

pub fn get_filters<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let array = dobj
            .filters()
            .into_iter()
            .map(|f| f.as_avm2_object(activation))
            .collect::<Result<ArrayStorage<'gc>, Error<'gc>>>()?;
        return Ok(ArrayObject::from_storage(activation, array)?.into());
    }
    Ok(ArrayObject::empty(activation)?.into())
}

fn build_argument_type_error<'gc>(
    activation: &mut Activation<'_, 'gc>,
) -> Result<Value<'gc>, Error<'gc>> {
    Err(Error::AvmError(crate::avm2::error::argument_error(
        activation,
        "Error #2005: Parameter 0 is of the incorrect type. Should be type Filter.",
        2005,
    )?))
}

pub fn set_filters<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_filters = args.try_get_object(activation, 0);

        if let Some(new_filters) = new_filters {
            if let Some(filters_array) = new_filters.as_array_object() {
                if let Some(filters_storage) = filters_array.as_array_storage() {
                    let filter_class_object = activation.avm2().classes().bitmapfilter;
                    let filter_class = filter_class_object.inner_class_definition();
                    let mut filter_vec = Vec::with_capacity(filters_storage.length());

                    for filter in filters_storage.iter().flatten() {
                        if matches!(filter, Value::Undefined | Value::Null) {
                            return build_argument_type_error(activation);
                        } else {
                            let filter_object = filter.coerce_to_object(activation)?;

                            if !filter_object.is_of_type(filter_class, &mut activation.context) {
                                return build_argument_type_error(activation);
                            }

                            filter_vec.push(Filter::from_avm2_object(activation, filter_object)?);
                        }
                    }

                    dobj.set_filters(activation.context.gc_context, filter_vec);
                }
            }
        } else {
            dobj.set_filters(activation.context.gc_context, vec![]);
        }
    }

    Ok(Value::Undefined)
}

/// Implements `x`'s getter.
pub fn get_x<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.x().to_pixels().into());
    }

    Ok(Value::Undefined)
}

/// Implements `x`'s setter.
pub fn set_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let x = args.get_f64(activation, 0)?;
        dobj.set_x(activation.context.gc_context, Twips::from_pixels(x));
    }

    Ok(Value::Undefined)
}

/// Implements `y`'s getter.
pub fn get_y<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.y().to_pixels().into());
    }

    Ok(Value::Undefined)
}

/// Implements `y`'s setter.
pub fn set_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let y = args.get_f64(activation, 0)?;
        dobj.set_y(activation.context.gc_context, Twips::from_pixels(y));
    }

    Ok(Value::Undefined)
}

pub fn get_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "z");
    Ok(0.into())
}

pub fn set_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "z");
    Ok(Value::Undefined)
}

pub fn get_rotation_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "rotationX");
    Ok(0.into())
}

pub fn set_rotation_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "rotationX");
    Ok(Value::Undefined)
}

pub fn get_rotation_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "rotationY");
    Ok(0.into())
}

pub fn set_rotation_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "rotationY");
    Ok(Value::Undefined)
}

pub fn get_rotation_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "rotationZ");
    Ok(0.into())
}

pub fn set_rotation_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "rotationZ");
    Ok(Value::Undefined)
}

pub fn get_scale_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_getter!(activation, "flash.display.DisplayObject", "scaleZ");
    Ok(1.into())
}

pub fn set_scale_z<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm2_stub_setter!(activation, "flash.display.DisplayObject", "scaleZ");
    Ok(Value::Undefined)
}

/// Implements `rotation`'s getter.
pub fn get_rotation<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let rot: f64 = dobj.rotation(activation.context.gc_context).into();
        let rem = rot % 360.0;

        if rem <= 180.0 {
            return Ok(rem.into());
        } else {
            return Ok((rem - 360.0).into());
        }
    }

    Ok(Value::Undefined)
}

/// Implements `rotation`'s setter.
pub fn set_rotation<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_rotation = args.get_f64(activation, 0)?;

        dobj.set_rotation(activation.context.gc_context, Degrees::from(new_rotation));
    }

    Ok(Value::Undefined)
}

/// Implements `name`'s getter.
pub fn get_name<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.name().into());
    }

    Ok(Value::Undefined)
}

/// Implements `name`'s setter.
pub fn set_name<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_name = args.get_string(activation, 0)?;

        if dobj.instantiated_by_timeline() {
            return Err(Error::AvmError(illegal_operation_error(
                activation,
                "Error #2078: The name property of a Timeline-placed object cannot be modified.",
                2078,
            )?));
        }

        dobj.set_name(activation.context.gc_context, new_name);
    }

    Ok(Value::Undefined)
}

/// Implements `parent`.
pub fn get_parent<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj
            .avm2_parent()
            .map(|parent| parent.object2())
            .unwrap_or(Value::Null));
    }

    Ok(Value::Undefined)
}

/// Implements `root`.
pub fn get_root<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj
            .avm2_root()
            .map(|root| root.object2())
            .unwrap_or(Value::Null));
    }

    Ok(Value::Undefined)
}

/// Implements `stage`.
pub fn get_stage<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj
            .avm2_stage(&activation.context)
            .map(|stage| stage.object2())
            .unwrap_or(Value::Null));
    }

    Ok(Value::Undefined)
}

/// Implements `visible`'s getter.
pub fn get_visible<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        return Ok(dobj.visible().into());
    }

    Ok(Value::Undefined)
}

/// Implements `visible`'s setter.
pub fn set_visible<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let new_visible = args.get_bool(0);

        dobj.set_visible(activation.context.gc_context, new_visible);
    }

    Ok(Value::Undefined)
}

/// Implements `mouseX`.
pub fn get_mouse_x<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let local_mouse = dobj.mouse_to_local(*activation.context.mouse_position);
        return Ok(local_mouse.x.to_pixels().into());
    }

    Ok(Value::Undefined)
}

/// Implements `mouseY`.
pub fn get_mouse_y<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let local_mouse = dobj.mouse_to_local(*activation.context.mouse_position);
        return Ok(local_mouse.y.to_pixels().into());
    }

    Ok(Value::Undefined)
}

/// Implements `hitTestPoint`.
pub fn hit_test_point<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let x = args.get_f64(activation, 0)?;
        let y = args.get_f64(activation, 1)?;
        let shape_flag = args.get_bool(2);

        // Transform the coordinates from root to world space.
        let local = Point::from_pixels(x, y);
        let global = dobj
            .avm2_root()
            .map_or(local, |root| root.local_to_global(local));

        if shape_flag {
            if !dobj.is_on_stage(&activation.context) {
                return Ok(false.into());
            }

            return Ok(dobj
                .hit_test_shape(
                    &mut activation.context,
                    global,
                    HitTestOptions::AVM_HIT_TEST,
                )
                .into());
        } else {
            return Ok(dobj.hit_test_bounds(global).into());
        }
    }

    Ok(Value::Undefined)
}

/// Implements `hitTestObject`.
pub fn hit_test_object<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        if let Some(rhs_dobj) = args.get_object(activation, 0, "obj")?.as_display_object() {
            return Ok(dobj.hit_test_object(rhs_dobj).into());
        }
    }

    Ok(Value::Undefined)
}

/// Implements `loaderInfo` getter
pub fn get_loader_info<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        // Contrary to the DisplayObject.loaderInfo documentation,
        // Flash Player defines 'loaderInfo' for non-root DisplayObjects.
        // It always returns the LoaderInfo from the root object.
        if let Some(loader_info) = dobj
            .avm2_root()
            .and_then(|root_dobj| root_dobj.loader_info())
        {
            return Ok(loader_info.into());
        }
        return Ok(Value::Null);
    }
    Ok(Value::Undefined)
}

pub fn get_transform<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(activation
        .avm2()
        .classes()
        .transform
        .construct(activation, &[this.into()])?
        .into())
}

pub fn set_transform<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let transform = args.get_object(activation, 0, "transform")?;

    // FIXME - consider 3D matrix and pixel bounds
    let matrix = transform
        .get_public_property("matrix", activation)?
        .coerce_to_object(activation)?;
    let color_transform = transform
        .get_public_property("colorTransform", activation)?
        .coerce_to_object(activation)?;

    let matrix =
        crate::avm2::globals::flash::geom::transform::object_to_matrix(matrix, activation)?;
    let color_transform = crate::avm2::globals::flash::geom::transform::object_to_color_transform(
        color_transform,
        activation,
    )?;

    let dobj = this.as_display_object().unwrap();
    let mut write = dobj.base_mut(activation.context.gc_context);
    write.set_matrix(matrix);
    write.set_color_transform(color_transform);
    drop(write);
    if let Some(parent) = dobj.parent() {
        // Self-transform changes are automatically handled,
        // we only want to inform ancestors to avoid unnecessary invalidations for tx/ty
        parent.invalidate_cached_bitmap(activation.context.gc_context);
    }

    Ok(Value::Undefined)
}

/// Implements `DisplayObject.blendMode`'s getter.
pub fn get_blend_mode<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let mode =
            AvmString::new_utf8(activation.context.gc_context, dobj.blend_mode().to_string());
        return Ok(mode.into());
    }
    Ok(Value::Undefined)
}

/// Implements `DisplayObject.blendMode`'s setter.
pub fn set_blend_mode<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let mode = args.get_string(activation, 0)?;

        if let Ok(mode) = ExtendedBlendMode::from_str(&mode.to_string()) {
            dobj.set_blend_mode(activation.context.gc_context, mode);
        } else {
            tracing::error!("Unknown blend mode {}", mode);
            return Err(make_error_2008(activation, "blendMode"));
        }
    }
    Ok(Value::Undefined)
}

fn new_rectangle<'gc>(
    activation: &mut Activation<'_, 'gc>,
    rectangle: Rectangle<Twips>,
) -> Result<Object<'gc>, Error<'gc>> {
    let x = rectangle.x_min.to_pixels();
    let y = rectangle.y_min.to_pixels();
    let width = rectangle.width().to_pixels();
    let height = rectangle.height().to_pixels();
    let args = &[x.into(), y.into(), width.into(), height.into()];
    activation
        .avm2()
        .classes()
        .rectangle
        .construct(activation, args)
}

pub fn get_scroll_rect<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        if dobj.has_scroll_rect() {
            return Ok(new_rectangle(activation, dobj.next_scroll_rect())?.into());
        } else {
            return Ok(Value::Null);
        }
    }
    Ok(Value::Undefined)
}

pub fn object_to_rectangle<'gc>(
    activation: &mut Activation<'_, 'gc>,
    object: Object<'gc>,
) -> Result<Rectangle<Twips>, Error<'gc>> {
    const NAMES: &[&str] = &["x", "y", "width", "height"];
    let mut values = [0.0; 4];
    for (&name, value) in NAMES.iter().zip(&mut values) {
        *value = object
            .get_public_property(name, activation)?
            .coerce_to_number(activation)?;
    }
    let [x, y, width, height] = values;
    Ok(Rectangle {
        x_min: Twips::from_pixels_i32(round_to_even(x)),
        y_min: Twips::from_pixels_i32(round_to_even(y)),
        x_max: Twips::from_pixels_i32(round_to_even(x + width)),
        y_max: Twips::from_pixels_i32(round_to_even(y + height)),
    })
}

pub fn set_scroll_rect<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        if let Some(rectangle) = args.try_get_object(activation, 0) {
            // Flash only updates the "internal" scrollRect used by `localToLocal` when the next
            // frame is rendered. However, accessing `DisplayObject.scrollRect` from ActionScript
            // will immediately return the updated value.
            //
            // To implement this, our `DisplayObject.scrollRect` ActionScript getter/setter both
            // operate on a `next_scroll_rect` field. Just before we render a DisplayObject, we copy
            // its `next_scroll_rect` to the `scroll_rect` field used for both rendering and
            // `localToGlobal`.
            dobj.set_next_scroll_rect(
                activation.context.gc_context,
                object_to_rectangle(activation, rectangle)?,
            );

            // TODO: Technically we should accept only `flash.geom.Rectangle` objects, in which case
            // `object_to_rectangle` will be infallible. Once this happens, the following line can
            // be moved above the `set_next_scroll_rect` call.
            dobj.set_has_scroll_rect(activation.context.gc_context, true);
        } else {
            dobj.set_has_scroll_rect(activation.context.gc_context, false);
        }
    }
    Ok(Value::Undefined)
}

pub fn local_to_global<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let point = args.get_object(activation, 0, "point")?;
        let x = point
            .get_public_property("x", activation)?
            .coerce_to_number(activation)?;
        let y = point
            .get_public_property("y", activation)?
            .coerce_to_number(activation)?;

        let local = Point::from_pixels(x, y);
        let global = dobj.local_to_global(local);
        return Ok(activation
            .avm2()
            .classes()
            .point
            .construct(
                activation,
                &[global.x.to_pixels().into(), global.y.to_pixels().into()],
            )?
            .into());
    }

    Ok(Value::Undefined)
}

pub fn global_to_local<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let point = args.get_object(activation, 0, "point")?;
        let x = point
            .get_public_property("x", activation)?
            .coerce_to_number(activation)?;
        let y = point
            .get_public_property("y", activation)?
            .coerce_to_number(activation)?;

        let global = Point::from_pixels(x, y);
        let local = dobj.global_to_local(global).unwrap_or(global);
        return Ok(activation
            .avm2()
            .classes()
            .point
            .construct(
                activation,
                &[local.x.to_pixels().into(), local.y.to_pixels().into()],
            )?
            .into());
    }

    Ok(Value::Undefined)
}

pub fn get_bounds<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let target = args
            .try_get_object(activation, 0)
            .and_then(|o| o.as_display_object())
            .unwrap_or(dobj);
        let bounds = dobj.bounds();
        let out_bounds = if DisplayObject::ptr_eq(dobj, target) {
            // Getting the clips bounds in its own coordinate space; no AABB transform needed.
            bounds
        } else {
            // Transform AABB to target space.
            // Calculate the matrix to transform into the target coordinate space, and transform the above AABB.
            // Note that this doesn't produce as tight of an AABB as if we had used `bounds_with_transform` with
            // the final matrix, but this matches Flash's behavior.
            let to_global_matrix = dobj.local_to_global_matrix();
            let to_target_matrix = target.global_to_local_matrix().unwrap_or_default();
            to_target_matrix * to_global_matrix * bounds
        };

        return Ok(new_rectangle(activation, out_bounds)?.into());
    }
    Ok(Value::Undefined)
}

pub fn get_rect<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // TODO: This should get the bounds ignoring strokes. Always equal to or smaller than getBounds.
    // Just defer to getBounds for now. Will have to store edge_bounds vs. shape_bounds in Graphic.
    get_bounds(activation, this, args)
}

pub fn get_mask<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(this) = this.as_display_object() {
        return Ok(this.masker().map_or(Value::Null, |m| m.object2()));
    }
    Ok(Value::Undefined)
}

pub fn set_mask<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(this) = this.as_display_object() {
        let mask = args.try_get_object(activation, 0);

        if let Some(mask) = mask {
            let mask = mask.as_display_object().ok_or_else(|| -> Error {
                format!("Mask is not a DisplayObject: {mask:?}").into()
            })?;

            this.set_masker(activation.context.gc_context, Some(mask), true);
            mask.set_maskee(activation.context.gc_context, Some(this), true);
        } else {
            this.set_masker(activation.context.gc_context, None, true);
        }
    }
    Ok(Value::Undefined)
}

pub fn get_cache_as_bitmap<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(this) = this.as_display_object() {
        return Ok(this.is_bitmap_cached().into());
    }
    Ok(Value::Undefined)
}

pub fn set_cache_as_bitmap<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(this) = this.as_display_object() {
        let cache = args.get(0).unwrap_or(&Value::Undefined).coerce_to_boolean();
        this.set_bitmap_cached_preference(activation.context.gc_context, cache);
    }
    Ok(Value::Undefined)
}

/// `opaqueBackground`'s getter.
pub fn get_opaque_background<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(color) = this
        .as_display_object()
        .and_then(|this| this.opaque_background())
    {
        return Ok(color.to_rgb().into());
    }

    Ok(Value::Null)
}

/// `opaqueBackground`'s setter.
pub fn set_opaque_background<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let value = args.get(0).unwrap_or(&Value::Undefined);
        let color = match value {
            Value::Null | Value::Undefined => None,
            value => Some(Color::from_rgb(value.coerce_to_u32(activation)?, 255)),
        };
        dobj.set_opaque_background(activation.context.gc_context, color);
    }

    Ok(Value::Undefined)
}

pub fn set_blend_shader<'gc>(
    activation: &mut Activation<'_, 'gc>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(dobj) = this.as_display_object() {
        let Some(shader_data) = args
            .get_object(activation, 0, "shader")?
            .get_public_property("data", activation)?
            .as_object()
        else {
            return Err(Error::AvmError(type_error(
                activation,
                "Error #2007: Parameter data must be non-null.",
                2007,
            )?));
        };

        let shader_handle = shader_data
            .as_shader_data()
            .expect("Shader.data is not a ShaderData instance")
            .pixel_bender_shader()
            .expect("Missing compiled PixelBender shader");

        dobj.set_blend_shader(activation.context.gc_context, Some(shader_handle));
    }
    Ok(Value::Undefined)
}
