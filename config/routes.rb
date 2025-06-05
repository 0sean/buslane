Rails.application.routes.draw do
  # Define your application routes per the DSL in https://guides.rubyonrails.org/routing.html

  # Reveal health status on /up that returns 200 if the app boots with no exceptions, otherwise 500.
  # Can be used by load balancers and uptime monitors to verify that the app is live.
  get "up" => "rails/health#show", as: :rails_health_check

  scope :api, module: nil do
    namespace :timetables do
      resources :agencies, only: [ :index, :show ] do
        collection do
          get "by_noc/:noc", action: :by_noc, as: :by_noc
        end
        resources :routes, only: [ :index ]
      end
      resources :routes, only: [ :index, :show ]
      resources :services, only: [ :show ]
    end
  end
end
