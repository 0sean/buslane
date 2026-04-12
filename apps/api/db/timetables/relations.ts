import { relations } from "drizzle-orm/relations";
import { stops, stopTimes, trips, agencies, routes, services, calendarDates, frequencies } from "./schema";

export const stopTimesRelations = relations(stopTimes, ({one}) => ({
	stop: one(stops, {
		fields: [stopTimes.stopId],
		references: [stops.id]
	}),
	trip: one(trips, {
		fields: [stopTimes.tripId],
		references: [trips.id]
	}),
}));

export const stopsRelations = relations(stops, ({many}) => ({
	stopTimes: many(stopTimes),
}));

export const tripsRelations = relations(trips, ({one, many}) => ({
	stopTimes: many(stopTimes),
	route: one(routes, {
		fields: [trips.routeId],
		references: [routes.id]
	}),
	service: one(services, {
		fields: [trips.serviceId],
		references: [services.id]
	}),
	frequencies: many(frequencies),
}));

export const routesRelations = relations(routes, ({one, many}) => ({
	agency: one(agencies, {
		fields: [routes.agencyId],
		references: [agencies.id]
	}),
	trips: many(trips),
}));

export const agenciesRelations = relations(agencies, ({many}) => ({
	routes: many(routes),
}));

export const servicesRelations = relations(services, ({many}) => ({
	trips: many(trips),
	calendarDates: many(calendarDates),
}));

export const calendarDatesRelations = relations(calendarDates, ({one}) => ({
	service: one(services, {
		fields: [calendarDates.serviceId],
		references: [services.id]
	}),
}));

export const frequenciesRelations = relations(frequencies, ({one}) => ({
	trip: one(trips, {
		fields: [frequencies.tripId],
		references: [trips.id]
	}),
}));