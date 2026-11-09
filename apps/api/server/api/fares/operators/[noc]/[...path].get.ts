import { defineHandler } from "nitro";
import { getRouterParam, HTTPError } from "h3";
import { fares } from "../../../../../utils/fares.ts";
import type { FaresQuery } from "../../../../../db/fares/repository.ts";
export default defineHandler((event) => {
  const noc = getRouterParam(event, "noc");
  const parts = (getRouterParam(event, "path") ?? "")
    .split("/")
    .filter(Boolean);
  const [resource, id, child] = parts;
  const query: FaresQuery = { kind: "catalogue", noc };
  if (
    parts.length === 1 &&
    ["areas", "tickets", "families", "routes", "changes", "geometry"].includes(
      resource ?? "",
    )
  )
    query.action = resource === "geometry" ? "operatorGeometry" : resource;
  else if (resource === "journey-fares" && parts.length === 1)
    query.action = "journey";
  else if (
    parts.length === 2 &&
    ["areas", "tickets", "families", "routes", "points"].includes(
      resource ?? "",
    )
  )
    Object.assign(query, { action: "detail", id });
  else if (
    parts.length === 3 &&
    ["areas", "tickets", "families", "points", "routes"].includes(
      resource ?? "",
    ) &&
    ["sources", "stops", "geometry"].includes(child ?? "")
  )
    Object.assign(query, { action: child, id });
  else if (
    parts.length === 3 &&
    resource === "tickets" &&
    ["areas", "routes", "families"].includes(child ?? "")
  )
    Object.assign(query, {
      action: "related",
      id,
      relation: (
        { areas: "area", routes: "route", families: "family" } as Record<
          string,
          string
        >
      )[child!],
    });
  else if (parts.length === 3 && resource === "areas" && child === "families")
    Object.assign(query, { action: "areaFamilies", id });
  else if (parts.length === 3 && resource === "areas" && child === "tickets")
    Object.assign(query, { action: "areaTickets", id });
  else if (parts.length === 3 && resource === "families" && child === "tickets")
    Object.assign(query, { action: "familyTickets", id });
  else if (
    parts.length === 3 &&
    resource === "routes" &&
    ["origins", "destinations"].includes(child ?? "")
  )
    Object.assign(query, { action: child, route_id: id });
  else
    throw new HTTPError({
      status: 404,
      body: { error: "Unknown catalogue endpoint" },
    });
  return fares(event, query);
});
