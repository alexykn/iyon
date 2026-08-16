import { native } from "../native.ts";
import { asTuiError } from "./errors.ts";
import { nodeForMaterialization, type View } from "./values/view.ts";

export interface NativeMaterializedView {
  readonly __nativeView: unique symbol;
}

/** Internal render-boundary crossing. The recursive node is never public. */
export function materializeView(view: View): NativeMaterializedView | undefined {
  const materializer = native.materializeView;
  if (materializer === undefined) {
    return undefined;
  }
  try {
    return materializer(nodeForMaterialization(view)) as NativeMaterializedView;
  } catch (error) {
    throw asTuiError(error);
  }
}
